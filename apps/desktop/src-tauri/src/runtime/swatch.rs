//! Repère de couleurs d'un effet — prélevé **en l'exécutant**.
//!
//! Dans la bibliothèque, chaque effet porte quelques couleurs qui aident à le
//! retrouver sans le lancer. Elles sont échantillonnées sur le rendu, jamais
//! déclarées dans le manifeste ni dessinées à la main, pour deux raisons :
//!
//! 1. l'auteur n'a rien à fournir — on écrit son effet, il a son repère ;
//! 2. surtout, **le repère ne peut pas mentir**. Déclaré à la main, il
//!    dériverait dès la première modification du code : un effet devenu bleu
//!    garderait sa vignette rouge. Ici, il *vient* de l'effet.
//!
//! Le moteur sait déjà produire une image sans toucher au matériel —
//! [`super::prepare`] puis [`super::render_once`]. L'échantillonnage n'est que
//! quelques appels de plus, sur le même chemin que la production : ce qu'on
//! montre dans la liste est rendu par le code qui allumera le clavier.
//!
//! # Où l'on prélève, et pourquoi
//!
//! [`SAMPLES`] images, à des instants différents, et une couleur par image —
//! mais **pas au même endroit du clavier** d'une image à la suivante.
//!
//! Prendre une touche toujours à la même place ne distinguerait pas un dégradé
//! immobile d'un aplat uni : les deux rendraient quatre fois la même couleur.
//! Moyenner l'image entière ne les distinguerait pas davantage — la moyenne
//! réduit toute image à une seule teinte, et un arc-en-ciel moyenné est gris.
//!
//! Chaque prélèvement est donc la moyenne d'une **bande diagonale** du clavier,
//! et les bandes avancent d'une image à l'autre. Diagonale, et non une rangée
//! ni une colonne : un dégradé horizontal ne varie que selon la colonne, un
//! balayage vertical que selon la rangée. Découper selon l'une des deux rendrait
//! l'autre parfaitement uniforme — donc invisible dans le repère.
//!
//! Une bande, et non une touche : un effet peut laisser la majeure partie du
//! clavier éteinte — « Balayage » est exactement cela —, et une touche isolée
//! tomberait sur du noir par hasard. La moyenne d'un quart des LED, elle, dit
//! quelque chose de vrai : un effet majoritairement sombre donne un repère
//! sombre, et c'est précisément ce qui le distingue d'un effet qui remplit tout.
//!
//! Les instants sont **irrégulièrement espacés**. Régulièrement espacés, ils se
//! caleraient sur la période d'un effet cyclique et rendraient quatre fois la
//! même couleur — la panne même qu'on cherche à éviter.
//!
//! # Ce que le format ne fige pas
//!
//! Un repère est une **liste** de couleurs, pas un quadruplet. Le jour où la
//! galerie voudra des vignettes animées, il suffira de ne pas s'arrêter à
//! quelques images : ni le stockage ni le type exposé n'ont à changer.

use std::time::{Duration, Instant};

use candeo_device::Layout;
use rquickjs::Context;

/// Nombre de couleurs d'un repère.
///
/// Quatre : assez pour qu'un dégradé se lise comme un dégradé et qu'un cycle se
/// lise comme un cycle, assez peu pour que l'échantillonnage reste imperceptible
/// à l'installation.
pub const SAMPLES: usize = 4;

/// Instants de rendu, en secondes.
///
/// Irrégulièrement espacés : voir l'en-tête du module. Ils couvrent un peu plus
/// de deux secondes, ce qui laisse le temps à un effet lent — « Respiration »
/// respire en cinq secondes — de montrer autre chose que son image de départ.
const INSTANTS: [f64; SAMPLES] = [0.0, 0.37, 1.13, 2.61];

/// Temps maximal accordé à un échantillonnage complet, chargement compris.
///
/// C'est du code utilisateur : il peut lever, mais il peut aussi boucler sans
/// fin. Sans cette borne, un effet qui boucle **empêcherait son installation**
/// pour toujours — le contraire de ce qu'on veut d'un repère, qui n'est qu'un
/// agrément. Quelques millisecondes suffisent en pratique ; deux secondes sont
/// deux ordres de grandeur au-dessus.
const BUDGET: Duration = Duration::from_secs(2);

/// Le repère d'un effet : quelques couleurs `#rrggbb`.
///
/// Des chaînes hexadécimales plutôt que des triplets : l'interface les pose
/// telles quelles en CSS, et le fichier reste lisible quand on l'ouvre.
pub type Swatch = Vec<String>;

/// Échantillonne le repère d'un effet.
///
/// **N'échoue jamais.** Un effet qui ne charge pas, qui lève ou qui boucle rend
/// un repère vide ; c'est à l'appelant de retomber sur quelque chose de neutre.
/// Un repère manquant n'est pas une raison de refuser un effet par ailleurs
/// valide.
///
/// Un effet qui rend du noir partout, en revanche, n'est **pas** un échec : son
/// repère est noir, et c'est la vérité sur ce qu'il fait.
pub fn sample(js: &str, layout: &'static Layout) -> Swatch {
    let deadline = Instant::now() + BUDGET;

    let Ok((_rt, ctx)) = super::prepare_bounded(js, layout, Some(deadline)) else {
        return Swatch::new();
    };

    let params = default_params(&ctx);
    let bands = bands(layout);
    let frame_len = layout.led_count();

    let mut swatch = Swatch::with_capacity(SAMPLES);
    for (&time, band) in INSTANTS.iter().zip(&bands) {
        // L'index d'image est celui qu'aurait la boucle à cet instant : un effet
        // qui compte les images plutôt que les secondes doit avancer lui aussi.
        let frame_index = (time * f64::from(super::FPS)).round() as u32;

        // Une image qui lève est sautée, pas fatale : un effet qui ne trébuche
        // qu'à un instant garde les couleurs qu'on a pu prélever ailleurs.
        let Ok(bytes) = super::render_once(&ctx, time, frame_index, &params, frame_len) else {
            continue;
        };
        swatch.push(hex(average(&bytes, band)));
    }
    swatch
}

/// Les paramètres avec lesquels échantillonner : **les valeurs par défaut**
/// déclarées par l'effet lui-même.
///
/// Pas un objet vide : rien n'oblige un effet à se replier sur une valeur quand
/// un paramètre manque, et `params.couleur.r` sur `undefined` donne du noir. Le
/// repère montrerait alors un effet que personne ne verra jamais — la galerie,
/// elle, lance l'effet avec ses défauts.
///
/// Ils sont lus dans le manifeste que le **module** déclare, pas dans celui que
/// le Rust annonce : c'est le module qui fait foi, et cela laisse la fonction
/// n'avoir besoin que du JavaScript.
fn default_params(ctx: &Context) -> String {
    let declared: Option<String> = ctx.with(|ctx| ctx.globals().get("__candeo_manifest").ok());
    let Some(declared) = declared else {
        return "{}".into();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&declared) else {
        return "{}".into();
    };
    let Some(specs) = manifest.get("params").and_then(|p| p.as_object()) else {
        return "{}".into();
    };

    let values: serde_json::Map<String, serde_json::Value> = specs
        .iter()
        .filter_map(|(name, spec)| Some((name.clone(), spec.get("default")?.clone())))
        .collect();
    serde_json::Value::Object(values).to_string()
}

/// Découpe les positions allumées en [`SAMPLES`] bandes diagonales.
///
/// La diagonale est une coordonnée unique, `rangée + colonne`, chacune
/// normalisée : elle avance donc quand on descend **et** quand on va vers la
/// droite. Les positions sont triées le long de cette coordonnée puis coupées en
/// groupes de **même effectif**, et non en tranches de même largeur — aucune
/// bande ne peut alors se retrouver vide sur un gabarit dont les LED sont
/// inégalement réparties, et chaque prélèvement pèse le même nombre de LED.
///
/// Les égalités sont nombreuses (toute une anti-diagonale partage sa
/// coordonnée) : le tri est stable et le parcours se fait rangée par rangée, le
/// découpage est donc déterministe. Il le faut — le même effet doit donner le
/// même repère à chaque installation.
fn bands(layout: &'static Layout) -> Vec<Vec<u16>> {
    // Un gabarit d'une seule rangée ou d'une seule colonne ne divise pas par
    // zéro : le terme correspondant reste simplement nul.
    let last_row = f32::from(layout.rows.saturating_sub(1).max(1));
    let last_col = f32::from(layout.cols.saturating_sub(1).max(1));

    let mut lit: Vec<(f32, u16)> = Vec::with_capacity(layout.lit_count());
    for row in 0..layout.rows {
        for col in 0..layout.cols {
            let Some(index) = layout.at(row, col) else {
                continue;
            };
            lit.push((f32::from(row) / last_row + f32::from(col) / last_col, index));
        }
    }
    lit.sort_by(|a, b| a.0.total_cmp(&b.0));

    (0..SAMPLES)
        .map(|k| {
            let start = k * lit.len() / SAMPLES;
            let end = (k + 1) * lit.len() / SAMPLES;
            lit[start..end].iter().map(|&(_, index)| index).collect()
        })
        .collect()
}

/// Couleur moyenne d'une bande, sur une image brute.
///
/// Moyenne arithmétique des octets, sans correction gamma : ce qu'on montre est
/// une pastille de quelques pixels, pas une image à reproduire fidèlement, et
/// une bande noire doit rester noire.
fn average(bytes: &[u8], band: &[u16]) -> [u8; 3] {
    let mut sum = [0u32; 3];
    let mut counted = 0u32;

    for &index in band {
        let at = index as usize * 3;
        // Une image plus courte que le gabarit est déjà refusée par
        // `render_once` ; la garde protège d'un gabarit changé sous nos pieds.
        let Some(color) = bytes.get(at..at + 3) else {
            continue;
        };
        for (s, &c) in sum.iter_mut().zip(color) {
            *s += u32::from(c);
        }
        counted += 1;
    }

    if counted == 0 {
        return [0, 0, 0];
    }
    [
        (sum[0] / counted) as u8,
        (sum[1] / counted) as u8,
        (sum[2] / counted) as u8,
    ]
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> &'static Layout {
        &candeo_device::DEATHSTALKER_V2_PRO
    }

    fn est_du_rrggbb(c: &str) -> bool {
        c.len() == 7
            && c.starts_with('#')
            && c[1..]
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    }

    /// Chacun des effets livrés doit produire un repère complet : c'est la
    /// promesse de la galerie, et un intégré sans repère se verrait au premier
    /// lancement.
    #[test]
    fn chaque_effet_integre_produit_un_repere() {
        for b in &crate::builtins::ALL {
            let swatch = sample(b.js, layout());
            assert_eq!(swatch.len(), SAMPLES, "« {} » : repère incomplet", b.id);
            for c in &swatch {
                assert!(est_du_rrggbb(c), "« {} » : couleur « {c} »", b.id);
            }
        }
    }

    /// Un effet qui lève rend un repère vide plutôt que de faire échouer
    /// l'appelant. C'est du code utilisateur : il a le droit d'être cassé.
    #[test]
    fn un_effet_qui_leve_ne_donne_pas_de_repere() {
        let js = "export default { name: 'X', render() { throw new Error('boum') } }";
        assert!(sample(js, layout()).is_empty());
    }

    /// Le chargement peut échouer avant même le premier rendu.
    #[test]
    fn un_effet_qui_ne_charge_pas_ne_donne_pas_de_repere() {
        assert!(sample("ceci n'est pas du JavaScript {{{", layout()).is_empty());
    }

    /// Un effet qui boucle sans fin est **interrompu**, pas attendu. Sans cette
    /// borne, il suffirait d'un `while (true)` pour qu'une installation ne
    /// revienne jamais.
    #[test]
    fn un_effet_qui_boucle_est_interrompu() {
        let js = "export default { name: 'X', render() { for (;;) {} } }";

        let debut = Instant::now();
        let swatch = sample(js, layout());

        assert!(swatch.is_empty());
        assert!(
            debut.elapsed() < BUDGET * 3,
            "l'échantillonnage a duré {:?}",
            debut.elapsed()
        );
    }

    /// Le noir n'est pas un échec. Un effet « Éteint » a un repère, et il est
    /// noir — ce qui le distingue d'un effet dont le repère n'a pas pu être
    /// calculé, qui n'en a aucun.
    #[test]
    fn un_effet_tout_noir_a_un_repere_noir() {
        let js = r#"
            import { BLACK } from '@candeo/effects-api'
            export default { name: 'Éteint', render({ frame }) { frame.fill(BLACK) } }
        "#;
        assert_eq!(sample(js, layout()), vec!["#000000"; SAMPLES]);
    }

    /// Le cœur du sujet : un effet uniforme et un effet spatial ne doivent pas
    /// se ressembler. L'uniforme rend la même couleur partout et à tout instant,
    /// donc quatre fois la même ; le dégradé varie selon la colonne, donc quatre
    /// couleurs différentes. Prélever toujours au même endroit les confondrait.
    #[test]
    fn un_effet_uniforme_et_un_effet_spatial_ont_des_reperes_distincts() {
        let uniforme = r#"
            import { rgb } from '@candeo/effects-api'
            export default {
              name: 'Uni',
              render({ layout, frame }) {
                for (const key of layout.keys) frame.set(key, rgb(200, 40, 40))
              },
            }
        "#;
        let spatial = r#"
            import { mix } from '@candeo/effects-api'
            export default {
              name: 'Dégradé',
              render({ layout, frame }) {
                for (const key of layout.keys) {
                  frame.set(key, mix({ r: 200, g: 40, b: 40 }, { r: 40, g: 40, b: 200 }, key.col / (layout.cols - 1)))
                }
              },
            }
        "#;

        let uni = sample(uniforme, layout());
        let deg = sample(spatial, layout());

        assert_eq!(
            uni,
            vec!["#c82828"; SAMPLES],
            "l'aplat doit rester un aplat"
        );
        assert_eq!(deg.len(), SAMPLES);
        assert_eq!(
            deg.iter().collect::<std::collections::HashSet<_>>().len(),
            SAMPLES,
            "le dégradé doit donner quatre couleurs différentes : {deg:?}"
        );
        assert_ne!(uni, deg);
    }

    /// Deux effets livrés visuellement distincts donnent des repères distincts.
    /// C'est la même propriété que ci-dessus, vérifiée sur ce qu'on livre.
    #[test]
    fn deux_effets_integres_distincts_ont_des_reperes_distincts() {
        let respiration = sample(
            crate::builtins::find("respiration").expect("intégré").js,
            layout(),
        );
        let degrade = sample(
            crate::builtins::find("degrade-fixe").expect("intégré").js,
            layout(),
        );
        assert_ne!(respiration, degrade);
    }

    /// Le repère est calculé une fois et rangé : il doit donc être reproductible,
    /// sans quoi réenregistrer un effet inchangé en changerait la vignette.
    #[test]
    fn le_meme_effet_donne_toujours_le_meme_repere() {
        let js = crate::builtins::find("onde-radiale").expect("intégré").js;
        assert_eq!(sample(js, layout()), sample(js, layout()));
    }

    /// Les valeurs par défaut du module sont utilisées : sans elles, un effet
    /// qui ne se replie sur rien rendrait du noir, et son repère mentirait.
    #[test]
    fn les_parametres_sont_pris_a_leur_valeur_par_defaut() {
        let js = r#"
            export default {
              name: 'X',
              params: { color: { kind: 'color', label: 'Couleur', default: { r: 0, g: 255, b: 0 } } },
              render({ layout, frame, params }) {
                for (const key of layout.keys) frame.set(key, params.color)
              },
            }
        "#;
        assert_eq!(sample(js, layout()), vec!["#00ff00"; SAMPLES]);
    }

    /// Les bandes couvrent toutes les LED, une fois chacune, en parts égales à
    /// une unité près. Une bande vide donnerait du noir sans que rien ne le dise.
    #[test]
    fn les_bandes_partagent_toutes_les_led() {
        let bands = bands(layout());
        let total: usize = bands.iter().map(Vec::len).sum();
        assert_eq!(total, layout().lit_count());

        let mut vues: Vec<u16> = bands.concat();
        vues.sort_unstable();
        vues.dedup();
        assert_eq!(vues.len(), total, "une LED apparaît dans deux bandes");

        let plus_petite = bands.iter().map(Vec::len).min().unwrap();
        let plus_grande = bands.iter().map(Vec::len).max().unwrap();
        assert!(plus_grande - plus_petite <= 1, "bandes déséquilibrées");
    }
}
