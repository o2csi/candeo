//! Effets livrés avec l'application.
//!
//! Ce sont des **modules JavaScript**, chargés par le même moteur, contre la
//! même API et avec le même `export default` que les effets écrits par
//! l'utilisateur. Les écrire en Rust natif les rendrait plus rapides et ne
//! prouverait rien : le premier exemple qu'on ouvre doit être exactement ce
//! qu'on peut écrire soi-même.
//!
//! Ils n'ont pas de dossier : `include_str!` les compile dans le binaire. Leur
//! JavaScript est donc aussi leur source — il n'y a pas de `.ts` à transpiler,
//! et c'est ce qui permet de les lire tels qu'ils s'exécutent.
//!
//! ## Deux déclarations, un seul contenu
//!
//! Le manifeste est écrit ici, en Rust, pour que lister la bibliothèque ne
//! coûte pas l'instanciation de quatre contextes QuickJS ; le module, lui, le
//! déclare aussi dans son `export default`, parce que c'est le contrat de
//! l'API. Le test `runtime::tests::les_manifestes_integres_correspondent_aux_modules`
//! interdit la divergence : c'est le module qui fait foi.

/// Un effet compilé dans le binaire.
pub struct Builtin {
    /// Identifiant stable, écrit à la main et non dérivé du nom : il est
    /// enregistré dans `settings.json` comme effet actif, renommer l'effet ne
    /// doit donc pas le changer.
    pub id: &'static str,
    /// Le module, tel que le moteur le charge.
    pub js: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Paramètres déclarés, en JSON, à la forme de `ParamSpec` côté TypeScript.
    pub params: &'static str,
}

/// Les effets livrés, dans l'ordre où la galerie les présente.
///
/// Quatre, et volontairement pas davantage : ils sont là pour être lus. Deux
/// variantes d'un même mouvement n'apprendraient rien de plus et rendraient la
/// galerie moins lisible qu'elle ne l'est vide.
pub static ALL: [Builtin; 4] = [
    Builtin {
        id: "onde-radiale",
        js: include_str!("onde-radiale.js"),
        name: "Onde radiale",
        description: "Une onde de teinte se propage depuis le centre du clavier",
        params: r#"{
            "speed": { "kind": "number", "label": "Vitesse", "min": 0, "max": 400, "default": 120 },
            "scale": { "kind": "number", "label": "Échelle", "min": 1, "max": 60, "default": 18 }
        }"#,
    },
    Builtin {
        id: "respiration",
        js: include_str!("respiration.js"),
        name: "Respiration",
        description: "Tout le clavier respire, d'une seule couleur",
        params: r#"{
            "color": { "kind": "color", "label": "Couleur", "default": { "r": 255, "g": 96, "b": 0 } },
            "period": { "kind": "number", "label": "Période (s)", "min": 1, "max": 20, "step": 0.5, "default": 5 }
        }"#,
    },
    Builtin {
        id: "balayage",
        js: include_str!("balayage.js"),
        name: "Balayage",
        description: "Une rangée éclairée descend le clavier en laissant une traînée",
        params: r#"{
            "color": { "kind": "color", "label": "Couleur", "default": { "r": 0, "g": 180, "b": 255 } },
            "speed": { "kind": "number", "label": "Rangées par seconde", "min": 0.5, "max": 12, "step": 0.5, "default": 3 },
            "trail": { "kind": "number", "label": "Traînée (rangées)", "min": 0.5, "max": 6, "step": 0.5, "default": 2 },
            "bounce": { "kind": "boolean", "label": "Rebond", "default": false }
        }"#,
    },
    Builtin {
        id: "degrade-fixe",
        js: include_str!("degrade-fixe.js"),
        name: "Dégradé fixe",
        description: "Un dégradé entre deux couleurs, immobile",
        params: r#"{
            "from": { "kind": "color", "label": "Couleur de départ", "default": { "r": 255, "g": 0, "b": 128 } },
            "to": { "kind": "color", "label": "Couleur d'arrivée", "default": { "r": 0, "g": 128, "b": 255 } },
            "axis": { "kind": "choice", "label": "Sens", "options": ["horizontal", "vertical"], "default": "horizontal" }
        }"#,
    },
];

/// L'effet intégré portant cet identifiant, s'il existe.
///
/// C'est le point d'entrée unique de la priorité décrite dans
/// [`crate::storage`] : un identifiant intégré est résolu ici **avant** tout
/// accès au disque.
pub fn find(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|b| b.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un identifiant intégré vit dans le **même** espace de noms que ceux des
    /// effets utilisateur : il est enregistré dans les réglages et affiché
    /// comme les autres. S'il ne satisfaisait pas la même validation, la
    /// réservation faite à l'installation ne protégerait rien.
    #[test]
    fn les_identifiants_integres_sont_des_identifiants_valides() {
        for b in &ALL {
            crate::storage::validate_id(b.id).unwrap_or_else(|e| panic!("« {} » : {e}", b.id));
        }
    }

    #[test]
    fn les_identifiants_integres_sont_uniques() {
        for (i, b) in ALL.iter().enumerate() {
            assert!(
                ALL[i + 1..].iter().all(|o| o.id != b.id),
                "« {} » est livré deux fois",
                b.id
            );
        }
    }

    /// Un manifeste intégré est écrit à la main : un JSON invalide passerait la
    /// compilation et ne se verrait qu'à l'ouverture de la galerie.
    #[test]
    fn les_parametres_integres_sont_du_json_valide() {
        for b in &ALL {
            let params: serde_json::Value =
                serde_json::from_str(b.params).unwrap_or_else(|e| panic!("« {} » : {e}", b.id));
            assert!(
                params.as_object().is_some_and(|o| !o.is_empty()),
                "« {} » ne déclare aucun paramètre",
                b.id
            );
        }
    }
}
