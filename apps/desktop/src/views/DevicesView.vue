<script setup lang="ts">
/**
 * Devices: what is plugged in, what was decided for each, and its technical
 * details. What concerns the whole application lives in Settings.
 */

import { onMounted } from 'vue'

import { useDevice } from '../composables/useDevice'
import { t } from '../i18n'

const { devices, layout, busy, refresh, adopt, ignore } = useDevice()

onMounted(refresh)
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>{{ t('devices.title') }}</h1>
      <button class="ghost" :disabled="busy" @click="refresh">{{ t('devices.search') }}</button>
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
            <!--
              La version lue, en face de celle du relevé : c'est la première
              question devant un clavier qui n'obéit pas. Fermé, on dit qu'elle
              n'a pas été lue plutôt que de laisser un vide qui se lirait comme
              « aucune ».
            -->
            <span class="mono ids detail">
              {{
                t('devices.firmware', {
                  version:
                    d.firmware ??
                    (d.open ? t('devices.firmwareNotRead') : t('devices.firmwareNotReadClosed')),
                  surveyed: d.surveyedFirmware,
                })
              }}
            </span>
            <!--
              The layout is read from an open device, and a layout is a model's:
              its name is the device's. 132 and 106 are named apart, since
              confusing them is this hardware's trap: a frame covers all 132
              cells, not the 106 lit keys.
            -->
            <span v-if="d.open && layout?.name === d.name" class="mono ids detail">
              {{
                t('devices.matrix', {
                  rows: layout.rows,
                  cols: layout.cols,
                  frameLen: layout.frameLen,
                  keys: layout.keys.length,
                })
              }}
            </span>
          </div>

          <!--
            Deux pastilles, et elles ne disent pas la même chose : la première
            ce que voit le système, la seconde ce qui a été décidé. Les fondre
            en une seule rendrait « piloté mais débranché » indicible.
          -->
          <span class="tag" :class="d.present ? 'ok' : 'absent'">
            {{ d.present ? t('devices.plugged') : t('devices.unplugged') }}
          </span>
          <span class="tag" :class="d.state">{{ t(`devices.state.${d.state}`) }}</span>

          <!--
            Un appareil jamais vu est listé, pas piloté : c'est un bouton à
            cliquer une fois, pas une case à recocher à chaque lancement.

            Il reste proposé sur un appareil piloté, branché mais **fermé** : la
            décision peut viser un autre exemplaire du même modèle que celui qui
            est branché — sa série le dit à l'ouverture — et sans ce bouton le
            clavier branché ne pourrait plus être adopté qu'en passant par
            « Ignorer ».
          -->
          <button
            v-if="d.state !== 'adopted' || (d.present && !d.open)"
            class="solid"
            :disabled="busy"
            @click="adopt(d)"
          >
            {{ t('devices.control') }}
          </button>
          <button v-if="d.state !== 'ignored'" class="ghost" :disabled="busy" @click="ignore(d)">
            {{ t('devices.ignore') }}
          </button>
        </div>

        <!--
          L'erreur appartient à l'appareil qui l'a produite : affichée sur sa
          ligne, elle ne laisse pas croire que les autres sont touchés.
        -->
        <p v-if="d.error" class="err">{{ d.error }}</p>
        <!--
          Un avertissement, pas une erreur : rien n'est bloqué, l'appareil reste
          ouvert. Visible sur la ligne plutôt que dans le seul journal —
          une version différente de celle du relevé est la première piste devant
          un clavier qui n'obéit pas, et personne n'irait la chercher ailleurs.
        -->
        <p v-for="w in d.warnings" :key="w" class="warn" role="status">{{ w }}</p>
      </li>
    </ul>

    <p v-if="!devices.length" class="empty">{{ t('devices.none') }}</p>
    <p v-else class="note">{{ t('devices.neverSeen') }}</p>
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

/* Technical details, each on its own line under the identifier. */
.detail {
  display: block;
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

/* Un avertissement, pas une erreur : rien n'est cassé, mais rien ne l'éteindra. */
.warn {
  margin: 0;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}
</style>
