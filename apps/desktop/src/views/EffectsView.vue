<script setup lang="ts">
/**
 * Galerie d'effets — écran d'accueil.
 *
 * Trois natures, dans cet ordre. Le matériel vient **en premier** : c'est la
 * seule des trois qui ne coûte aucun temps processeur et qui survit à la
 * fermeture de l'application, donc souvent le bon choix. Le reléguer en bas de
 * liste le ferait passer pour un mode dégradé.
 *
 * Les deux autres dépendent d'un moteur qui n'existe pas encore. Elles sont
 * affichées vides et dites comme telles : une liste inventée serait plus
 * trompeuse qu'une liste absente.
 */
import { computed } from 'vue'
import { useRouter } from 'vue-router'

import EffectCard from '../components/EffectCard.vue'
import { useDevice } from '../composables/useDevice'
import { hardwareEffects, useEffects } from '../composables/useEffects'

const router = useRouter()
const { layout, busy } = useDevice()
const { applied, applying, error, apply } = useEffects()

/** `layout` n'est renseigné qu'une fois un périphérique réellement ouvert. */
const connected = computed(() => layout.value !== null)

/**
 * L'avertissement attend la fin de la recherche de périphériques lancée au
 * démarrage. Sans ce `busy`, il s'afficherait le temps de l'énumération puis
 * disparaîtrait : annoncer une absence qu'on n'a pas encore vérifiée.
 */
const noDevice = computed(() => !connected.value && !busy.value)
</script>

<template>
  <section class="page">
    <header class="head">
      <div class="head-text">
        <h1>Effets</h1>
        <p class="sub">Choisissez un effet, ou écrivez le vôtre.</p>
      </div>

      <button class="solid" @click="router.push('/editor')">
        <span class="plus" aria-hidden="true">＋</span>Nouvel effet
      </button>
    </header>

    <!--
      Sans clavier branché, la galerie reste consultable : on doit pouvoir voir
      ce que l'application propose avant de brancher quoi que ce soit. Mais
      l'écran ne peut pas rester inerte, il dit ce qui manque et où aller.
    -->
    <p v-if="noDevice" class="notice" role="status">
      Aucun clavier connecté : la galerie se parcourt, mais appliquer un effet demande un
      périphérique ouvert.
      <RouterLink to="/devices" class="link">Choisir un périphérique</RouterLink>
    </p>

    <p v-if="error" class="failure" role="alert">{{ error }}</p>

    <section class="group">
      <h2>Matériel</h2>
      <p class="group-sub">
        Exécutés par le micrologiciel du clavier. Aucun temps processeur, et ils continuent de
        tourner l'application fermée — machine éteinte comprise.
      </p>

      <ul class="grid">
        <li v-for="e in hardwareEffects" :key="e.id">
          <EffectCard
            :name="e.name"
            :summary="e.summary"
            :preview="e.preview"
            nature="Matériel"
            cost="Aucun temps processeur"
            persistence="Survit à la fermeture"
            :applied="applied === e.id"
            :busy="applying === e.id"
            :disabled="!connected || applying !== null"
            @select="apply(e)"
          />
        </li>
      </ul>

      <p class="hint">
        Rien n'est marqué appliqué au lancement : le protocole relevé sait écrire un effet, pas
        relire celui qui tourne. Le sens et la vitesse de <span class="mono">Wave</span> sont figés
        sur les seules valeurs observées à la capture ; leur plage réelle reste une question
        ouverte du relevé, et un réglage inventé ne serait pas un réglage.
      </p>
    </section>

    <section class="group">
      <h2>Intégrés</h2>
      <p class="group-sub">
        Livrés avec l'application et exécutés par la boucle hôte : ils s'arrêtent quand
        l'application se ferme.
      </p>
      <p class="pending">
        Aucun pour l'instant — le fil de rendu (issue&nbsp;#6) et les effets livrés (issue&nbsp;#11)
        restent à écrire. Rien n'est simulé ici : un effet affiché serait un effet qu'on ne peut
        pas lancer.
      </p>
    </section>

    <section class="group">
      <h2>À vous</h2>
      <p class="group-sub">
        Écrits dans l'éditeur. Même boucle hôte que les intégrés, mais conservés sur disque et
        relancés au démarrage.
      </p>
      <p class="pending">
        Aucun pour l'instant — ni l'éditeur (issue&nbsp;#10) ni le stockage (issue&nbsp;#4) ne sont
        en place. Le bouton «&nbsp;＋&nbsp;» ouvre déjà l'éditeur, aujourd'hui un emplacement
        réservé.
      </p>
    </section>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-5);
  padding: var(--gap-4);
  max-width: 960px;
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--gap-3);
}

.head-text {
  min-width: 0;
}

.sub {
  color: var(--text-muted);
}

.solid {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  padding: 6px var(--gap-3);
  background: var(--accent);
  color: var(--accent-ink);
  border-radius: var(--r-md);
  font-size: 13px;
  font-weight: 500;
}

.plus {
  font-size: 15px;
  line-height: 1;
}

.notice {
  padding: var(--gap-3);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 13px;
}

.link {
  margin-left: var(--gap-1);
  color: var(--accent);
  text-decoration: none;
  white-space: nowrap;
}

.link:hover {
  text-decoration: underline;
}

.failure {
  padding: var(--gap-3);
  background: color-mix(in srgb, var(--bad) 10%, transparent);
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
  font-size: 13px;
}

.group {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
}

.group-sub {
  max-width: 68ch;
  color: var(--text-muted);
  font-size: 13px;
}

.grid {
  display: grid;

  /* `min()` empêche la colonne d'imposer une largeur plancher supérieure à la
     fenêtre : sans lui, une fenêtre étroite déclencherait un défilement
     horizontal de toute la page. */
  grid-template-columns: repeat(auto-fill, minmax(min(220px, 100%), 1fr));
  gap: var(--gap-3);
  margin: var(--gap-2) 0 0;
  padding: 0;
  list-style: none;
}

.hint,
.pending {
  max-width: 68ch;
  color: var(--text-faint);
  font-size: 12px;
}

.pending {
  padding: var(--gap-3);
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-md);
}
</style>
