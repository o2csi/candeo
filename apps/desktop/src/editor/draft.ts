/**
 * Brouillons — ce qui empêche de perdre un effet en cours d'écriture.
 *
 * ## Le problème
 *
 * Tant qu'un effet n'est pas validé, il n'existe nulle part : `install_effect`
 * est le seul chemin vers le disque, et il demande du code qui compile. Or on
 * quitte l'éditeur bien avant d'en être là — un clic sur « Retour », une
 * fenêtre fermée, un rechargement à chaud en développement.
 *
 * ## Ce qui a été retenu
 *
 * Enregistrement continu dans le stockage local de la vue web, sous une clé par
 * effet, restauré à l'ouverture et effacé seulement après une installation
 * réussie.
 *
 * **Pourquoi pas une boîte de dialogue « voulez-vous enregistrer ? »** : elle
 * pose une question à laquelle on peut répondre de travers, et une seule fois.
 * Un brouillon restauré, lui, ne perd rien, ne demande rien, et se jette d'un
 * bouton quand on n'en veut plus.
 *
 * **Pourquoi pas un fichier sur disque** : il faudrait une commande Rust
 * d'écriture de brouillon. Le stockage de la vue web survit à la fermeture de
 * l'application comme à la navigation, ce qui couvre exactement les cas visés ;
 * la copie durable, elle, reste le `source.ts` écrit à l'installation.
 *
 * Le stockage peut être refusé — vue web durcie, profil en lecture seule. On
 * n'échoue pas pour autant : perdre un brouillon est ennuyeux, empêcher
 * d'écrire un effet le serait davantage.
 */

const PREFIX = 'candeo:brouillon:'

/** L'effet en cours d'écriture n'a pas encore d'identifiant : `null`. */
function key(id: string | null): string {
  return PREFIX + (id ?? '')
}

export function readDraft(id: string | null): string | null {
  try {
    return localStorage.getItem(key(id))
  } catch {
    return null
  }
}

export function writeDraft(id: string | null, source: string): void {
  try {
    localStorage.setItem(key(id), source)
  } catch {
    // Sans stockage, l'éditeur fonctionne — il ne rattrape simplement plus les
    // fausses manœuvres.
  }
}

export function clearDraft(id: string | null): void {
  try {
    localStorage.removeItem(key(id))
  } catch {
    // Voir `writeDraft`.
  }
}
