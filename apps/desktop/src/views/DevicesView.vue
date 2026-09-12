<script setup lang="ts">
/**
 * Périphériques — et, plus bas, la configuration qu'ils alimentent.
 *
 * Les deux sont sur le même écran parce que c'est le même fichier : les
 * décisions prises ici sont l'essentiel de `settings.json`, avec les réglages
 * retenus par paire appareil / effet. Remettre ce fichier au défaut se propose
 * donc là où on le remplit — et surtout **pas** dans la bibliothèque, où le
 * geste voisin efface du code écrit à la main.
 */

import { onMounted, ref } from 'vue'

import {
  diagnostic,
  getJournal,
  openLogDir,
  resetSettings,
  setLogLevel,
  type JournalStatus,
  type LogLevel,
} from '../api/candeo'
import { erreur } from '../api/journal'
import type { DeviceState } from '../api/types'
import { useDevice } from '../composables/useDevice'
import { useEffectParams } from '../composables/useEffectParams'
import { useEffects } from '../composables/useEffects'

const { devices, layout, busy, refresh, adopt, ignore } = useDevice()
const { dropAll } = useEffectParams()
const { forgetPosed } = useEffects()

/**
 * Les trois états, dans la langue de l'interface.
 *
 * Table plutôt que suite de ternaires : l'ajout d'un quatrième état ne doit pas
 * pouvoir être oublié ici, le compilateur le réclame.
 */
const LIBELLES: Record<DeviceState, string> = {
  adopted: 'Piloté',
  detected: 'Détecté',
  ignored: 'Ignoré',
}

/** La question est posée et attend sa réponse. */
const asking = ref(false)
const working = ref(false)
/** Ce qui a empêché la remise à zéro. Le message du Rust, tel quel. */
const problem = ref<string | null>(null)

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

/**
 * Remet `settings.json` au défaut.
 *
 * Le Rust fait tout ce qui doit l'être avant d'écrire : arrêter les boucles,
 * éteindre le rétroéclairage, refermer les appareils. Il ne touche à aucun effet
 * — et n'en a pas les moyens, la commande n'écrit que dans le fichier de
 * configuration.
 */
async function reset(): Promise<void> {
  problem.value = null
  working.value = true
  try {
    await resetSettings()

    // La fenêtre, elle, garde ce qu'elle avait lu. Sans ces deux oublis, le
    // premier mouvement de curseur réécrirait les réglages qu'on vient
    // d'effacer, et la galerie marquerait « actif » un effet matériel que le
    // Rust vient d'éteindre.
    dropAll()
    forgetPosed()

    asking.value = false
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    // Tout a changé d'un coup : état de chaque appareil, ouverture, erreurs —
    // et le niveau du journal, que le Rust vient de ramener au défaut.
    await refresh()
    await readJournal()
  }
}

// ---------------------------------------------------------------- journal

/**
 * Les cinq niveaux, dits par ce qu'ils apportent et non par leur nom technique.
 *
 * Table plutôt que suite de ternaires, comme pour les états d'appareil :
 * l'ajout d'un niveau ne doit pas pouvoir être oublié ici, le compilateur le
 * réclame.
 */
const NIVEAUX: Record<LogLevel, string> = {
  error: 'Erreurs seules',
  warn: 'Avertissements',
  info: 'Cycle de vie (défaut)',
  debug: 'Détaillé — par image',
  trace: 'Tout — par image',
}

const journal = ref<JournalStatus | null>(null)
/** Ce qui a empêché d'agir sur le journal. */
const journalProblem = ref<string | null>(null)
/** Le diagnostic, une fois demandé. Montré même quand la copie a réussi. */
const report = ref<string | null>(null)
/** Ce que la copie a donné, à dire en une ligne. */
const copied = ref<string | null>(null)

async function readJournal(): Promise<void> {
  try {
    journal.value = await getJournal()
  } catch (e) {
    journalProblem.value = message(e)
  }
}

/**
 * Change le niveau, **sans redémarrer**.
 *
 * Le Rust rend l'état qui en résulte plutôt que de nous laisser le deviner :
 * quand `CANDEO_LOG` impose le niveau, le réglage est écrit mais pas appliqué, et
 * seul le Rust sait le dire.
 */
async function chooseLevel(event: Event): Promise<void> {
  journalProblem.value = null
  try {
    journal.value = await setLogLevel((event.target as HTMLSelectElement).value as LogLevel)
  } catch (e) {
    journalProblem.value = message(e)
    // Le menu affiche désormais un niveau qui n'a pas été retenu.
    await readJournal()
  }
}

async function showLogs(): Promise<void> {
  journalProblem.value = null
  try {
    await openLogDir()
  } catch (e) {
    journalProblem.value = message(e)
  }
}

/**
 * Le diagnostic, copié **et** affiché.
 *
 * Les deux, parce que le presse-papiers peut refuser — le WebView le réserve aux
 * contextes sûrs, et rien ici ne garantit qu'il le soit partout. Un texte affiché
 * reste sélectionnable à la main ; un texte qu'on croit copié et qui ne l'est pas
 * se découvre au moment de coller, dans le formulaire de rapport de bogue.
 */
async function copyDiagnostic(): Promise<void> {
  journalProblem.value = null
  copied.value = null
  try {
    const texte = await diagnostic()
    report.value = texte
    try {
      await navigator.clipboard.writeText(texte)
      copied.value = 'Copié dans le presse-papiers.'
    } catch (e) {
      copied.value = 'Copie refusée par le système — le texte est ci-dessous, à sélectionner.'
      erreur('diagnostic', `presse-papiers indisponible : ${message(e)}`, e)
    }
  } catch (e) {
    journalProblem.value = message(e)
  }
}

onMounted(async () => {
  await refresh()
  await readJournal()
})
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>Périphériques</h1>
      <button class="ghost" :disabled="busy" @click="refresh">Rechercher</button>
    </header>

    <!--
      Un gabarit connu mais débranché reste affiché, marqué absent. Le masquer
      donnerait une liste vide, qui ressemble à une panne de l'application
      alors qu'il suffit de brancher le clavier.
    -->
    <ul class="list">
      <li
        v-for="d in devices"
        :key="`${d.vid}:${d.pid}`"
        class="row"
        :class="{ off: !d.present, ignored: d.state === 'ignored', open: d.open }"
      >
        <div class="main">
          <div class="id">
            <h2>{{ d.name }}</h2>
            <span class="mono ids">{{
              `${d.vid.toString(16).padStart(4, '0')}:${d.pid.toString(16).padStart(4, '0')}`
            }}</span>
          </div>

          <!--
            Deux pastilles, et elles ne disent pas la même chose : la première
            ce que voit le système, la seconde ce qui a été décidé. Les fondre
            en une seule rendrait « piloté mais débranché » indicible.
          -->
          <span class="tag" :class="d.present ? 'ok' : 'absent'">
            {{ d.present ? 'Branché' : 'Débranché' }}
          </span>
          <span class="tag" :class="d.state">{{ LIBELLES[d.state] }}</span>

          <!--
            Un appareil jamais vu est listé, pas piloté : c'est un bouton à
            cliquer une fois, pas une case à recocher à chaque lancement.
          -->
          <button v-if="d.state !== 'adopted'" class="solid" :disabled="busy" @click="adopt(d)">
            Piloter
          </button>
          <button v-if="d.state !== 'ignored'" class="ghost" :disabled="busy" @click="ignore(d)">
            Ignorer
          </button>
        </div>

        <!--
          L'erreur appartient à l'appareil qui l'a produite : affichée sur sa
          ligne, elle ne laisse pas croire que les autres sont touchés.
        -->
        <p v-if="d.error" class="err">{{ d.error }}</p>
      </li>
    </ul>

    <p v-if="!devices.length" class="empty">Aucun gabarit connu.</p>
    <p v-else class="note">
      Un appareil jamais vu est listé mais non piloté — la décision se prend une fois, puis se
      retient.
    </p>

    <!--
      132 et 106 sont affichés séparément, et nommés. Les confondre est le
      piège de ce matériel : une image doit couvrir les 132 cases, pas les 106
      touches, sinon les dernières rangées restent figées.
    -->
    <dl v-if="layout" class="facts">
      <div><dt>Matrice</dt><dd class="num">{{ layout.rows }} × {{ layout.cols }}</dd></div>
      <div><dt>Taille d'une image</dt><dd class="num">{{ layout.frameLen }}</dd></div>
      <div><dt>Touches éclairées</dt><dd class="num">{{ layout.keys.length }}</dd></div>
    </dl>

    <!--
      Le journal. Ici plutôt qu'ailleurs parce que c'est le même moment : on
      vient chercher pourquoi un clavier se comporte mal, on monte le niveau, on
      ouvre le dossier, on copie le diagnostic. Les trois gestes se suivent.
    -->
    <section v-if="journal" class="journal" aria-labelledby="journal-title">
      <h2 id="journal-title">Journal</h2>
      <p class="note">
        candeo écrit ce qu'il fait dans un fichier tournant, un par jour, sept au plus. C'est ce
        qu'on lit quand quelque chose se passe mal sans que l'écran le dise.
      </p>

      <p v-if="journalProblem" class="err" role="alert">{{ journalProblem }}</p>

      <div class="level">
        <label for="log-level">Niveau</label>
        <select
          id="log-level"
          :value="journal.setting ?? 'info'"
          :disabled="busy"
          @change="chooseLevel"
        >
          <option v-for="(libelle, niveau) in NIVEAUX" :key="niveau" :value="niveau">
            {{ libelle }}
          </option>
        </select>
        <span class="note">Appliqué tout de suite, sans relancer l'application.</span>
      </div>

      <!--
        ⚠️ Rendre visible qu'un niveau élevé est actif. Il porte du par-image :
        laissé en place et oublié, il remplit le disque en silence, et la
        rotation plafonne le nombre de fichiers, pas la taille de celui du jour.
        Il survit au redémarrage — c'est voulu, pour qui traque un défaut au
        lancement — donc rien ne le désactivera tout seul.
      -->
      <p v-if="journal.verbose" class="warn" role="status">
        Niveau détaillé actif : le journal enregistre chaque image et grossit vite. Il
        <strong>reste actif après un redémarrage</strong> — le ramener à « cycle de vie » une fois
        le relevé terminé.
      </p>

      <!--
        La variable d'environnement l'emporte, toujours : c'est ce qui permet de
        diagnostiquer une application qui ne va pas assez loin pour lire ses
        réglages. Le dire, plutôt que de laisser croire qu'un réglage sans effet
        a été pris en compte.
      -->
      <p v-if="journal.forcedByEnv" class="note" role="status">
        CANDEO_LOG impose le niveau {{ journal.level ?? 'demandé' }} pour cette exécution. Le réglage
        ci-dessus est retenu et s'appliquera au prochain lancement sans cette variable.
      </p>

      <div class="actions">
        <button class="ghost" :disabled="!journal.dir" @click="showLogs">
          Ouvrir le dossier des journaux
        </button>
        <button class="ghost" @click="copyDiagnostic">Copier le diagnostic</button>
      </div>

      <p v-if="journal.dir" class="mono path">{{ journal.dir }}</p>
      <p v-else class="err">Aucun fichier : le journal n'a pas pu ouvrir son dossier.</p>

      <p v-if="copied" class="note" role="status">{{ copied }}</p>
      <!--
        Affiché même quand la copie a réussi : un texte qu'on croit copié et qui
        ne l'est pas se découvre au moment de coller, dans le formulaire de
        rapport de bogue.
      -->
      <pre v-if="report" class="report">{{ report }}</pre>
    </section>

    <!--
      La configuration, et elle seule. Le dire ici est ce qui empêche de
      confondre « je désadopte un clavier » et « je vide ce que j'ai écrit » :
      les effets sont une bibliothèque, pas un réglage, et ils se suppriment un
      par un depuis la galerie.
    -->
    <section class="config" aria-labelledby="config-title">
      <h2 id="config-title">Configuration</h2>
      <p class="note">
        candeo retient les décisions prises ci-dessus et les réglages de chaque effet, appareil par
        appareil. C'est par là qu'on repasse quand quelque chose se comporte mal, et c'est ce qui
        rend un rapport de bogue exploitable : voilà ce qui se passe en repartant du défaut.
      </p>

      <p v-if="problem" class="err" role="alert">{{ problem }}</p>

      <!--
        Le bouton reste en place, et actif, pendant que la question est posée :
        le masquer retirerait le focus du clavier au moment précis où il doit
        atteindre la réponse, qui suit dans le document.
      -->
      <button class="ghost danger" :disabled="busy || working" @click="asking = true">
        Remettre la configuration au défaut
      </button>

      <!--
        La réponse est la tabulation suivante. Le titre porte `role="alert"` :
        l'encart apparaît sans que rien ne bouge à l'écran, il faut l'annoncer.
      -->
      <div v-if="asking" class="confirm" role="group" aria-labelledby="confirm-reset">
        <p id="confirm-reset" class="confirm-title" role="alert">
          Repartir de la configuration par défaut ?
        </p>
        <!--
          Ce qui part, énuméré — et ce qui ne part pas, dit aussi clairement. La
          dernière ligne est la plus importante des quatre.
        -->
        <ul class="what">
          <li>
            Tous les appareils repassent en <strong>détecté</strong> : plus aucun n'est ouvert au
            démarrage.
          </li>
          <li>
            Les effets en cours s'arrêtent et le rétroéclairage s'éteint, plutôt que de rester figé
            sur la dernière image.
          </li>
          <li>Les réglages retenus pour chaque effet, sur chaque appareil, sont oubliés.</li>
          <li>
            <strong>Vos effets ne sont pas touchés.</strong> Les supprimer est une autre action, une
            par effet, depuis la bibliothèque.
          </li>
        </ul>
        <div class="confirm-actions">
          <button class="solid danger" :disabled="working" @click="reset">
            Remettre au défaut
          </button>
          <button class="ghost" :disabled="working" @click="asking = false">Annuler</button>
        </div>
      </div>
    </section>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-4);
  padding: var(--gap-4);
  max-width: 720px;
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-3);
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-3) var(--gap-4);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.main {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
}

.row.off {
  background: none;
  border-style: dashed;
}

/* Ignoré : présent dans la liste, mais visiblement mis de côté. */
.row.ignored {
  opacity: 0.6;
}

/*
 * Ouvert *en ce moment* — pas « adopté ». C'est le signe qui manquait : un effet
 * qui tourne sans qu'aucun octet n'atteigne le clavier ne se voyait nulle part.
 */
.row.open {
  border-color: var(--accent);
}

.id {
  flex: 1;
  min-width: 0;
}

.err {
  margin: 0;
  color: var(--bad);
  font-size: 12px;
}

.ids {
  color: var(--text-faint);
  font-size: 12px;
}

.tag {
  padding: 2px var(--gap-2);
  border-radius: 99px;
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.tag.ok {
  color: var(--ok);
  background: color-mix(in srgb, var(--ok) 14%, transparent);
}

.tag.absent {
  color: var(--text-faint);
  background: var(--raised-2);
}

.tag.adopted {
  color: var(--accent);
  background: var(--accent-soft);
}

.tag.detected {
  color: var(--text-muted);
  background: var(--raised-2);
}

.tag.ignored {
  color: var(--text-faint);
  border: 1px solid var(--line-strong);
}

.solid,
.ghost {
  padding: 6px var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.solid {
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 500;
}

.ghost {
  border: 1px solid var(--line-strong);
  color: var(--text-muted);
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.solid:disabled,
.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.empty,
.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-5);
  margin: 0;
  padding-top: var(--gap-4);
  border-top: 1px solid var(--line);
}

.journal {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--gap-2);
  padding-top: var(--gap-4);
  border-top: 1px solid var(--line);
}

.level {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

.level select {
  padding: 5px var(--gap-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  background: var(--raised);
  color: var(--text);
  font-size: 13px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

/* Un avertissement, pas une erreur : rien n'est cassé, mais rien ne l'éteindra. */
.warn {
  margin: 0;
  align-self: stretch;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}

.path {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
  overflow-wrap: anywhere;
}

/*
 * Le diagnostic reste sélectionnable : c'est le repli quand le presse-papiers
 * refuse, et la seule façon de vérifier ce qu'on s'apprête à publier.
 */
.report {
  align-self: stretch;
  max-height: 240px;
  margin: 0;
  padding: var(--gap-3);
  overflow: auto;
  background: var(--raised-2);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  user-select: text;
}

/* En dernier, et séparée : ce qui s'y trouve ne se reprend pas. */
.config {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--gap-2);
  padding-top: var(--gap-4);
  border-top: 1px solid var(--line);
}

/*
 * Un geste sans retour. La couleur ne le dit pas seule — le libellé l'annonce,
 * et la confirmation énumère ce qui part.
 */
.danger {
  color: var(--bad);
  border-color: var(--bad);
}

.solid.danger {
  background: var(--bad);
  color: var(--accent-ink);
}

.ghost.danger:hover:not(:disabled) {
  color: var(--bad);
  background: color-mix(in srgb, var(--bad) 12%, transparent);
}

.confirm {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  align-self: stretch;
  padding: var(--gap-3);
  background: color-mix(in srgb, var(--bad) 8%, var(--raised));
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
}

.confirm-title {
  font-weight: 600;
}

.what {
  margin: 0;
  padding-left: var(--gap-4);
  color: var(--text-muted);
  font-size: 13px;
}

.confirm-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

dt {
  color: var(--text-faint);
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

dd {
  margin: 2px 0 0;
  font-size: 18px;
}
</style>
