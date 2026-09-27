<script setup lang="ts">
/**
 * Games (`docs/design/game-state-integration.md` §4): a card per game — where
 * it stands, connecting it, what it says now, and what plays during a match.
 *
 * The tab shows only once games are turned on in Settings.
 */

import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'

import {
  connectGame,
  disconnectGame,
  getSettings,
  listEffects,
  listGames,
  listSignals,
  onSignalsChanged,
  setRules,
  type EffectEntry,
  type GameView,
  type HeldSignal,
  type Rule,
} from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceInfo } from '../api/types'
import FailureNote from '../components/FailureNote.vue'
import { gameRule, gameSignals, playing, withGameRule } from '../composables/games'
import { signalText } from '../composables/signals'
import { ids } from '../composables/devicesPage'
import { useDevice } from '../composables/useDevice'
import { t } from '../i18n'

const { devices } = useDevice()

const games = ref<GameView[]>([])
const held = ref<HeldSignal[]>([])
const rules = ref<Rule[]>([])
const effects = ref<EffectEntry[]>([])
const busy = ref(false)
const problem = ref<string | null>(null)
/** The game just connected, which reads its file when it starts. */
const restart = ref<string | null>(null)
let unlisten: UnlistenFn | null = null

/** What *while playing* can light: the devices Candeo controls. */
const controlled = computed(() => devices.value.filter((d) => d.state === 'adopted'))

async function readSignals(): Promise<void> {
  held.value = await listSignals().catch(() => held.value)
}

async function load(): Promise<void> {
  try {
    const [found, settings, library] = await Promise.all([listGames(), getSettings(), listEffects()])
    games.value = found
    rules.value = settings.rules
    effects.value = library.filter((e) => e.state === 'ready')
    await readSignals()
  } catch (e) {
    problem.value = message(e)
  }
}

async function act(task: () => Promise<void>): Promise<void> {
  busy.value = true
  problem.value = null
  try {
    await task()
  } catch (e) {
    problem.value = message(e)
  } finally {
    busy.value = false
  }
}

const connect = (game: GameView) =>
  act(async () => {
    games.value = await connectGame(game.id)
    restart.value = game.name
  })

const disconnect = (game: GameView) =>
  act(async () => {
    games.value = await disconnectGame(game.id)
    restart.value = null
  })

/** Where it stands, *playing* while a match sends its values. */
function stateOf(game: GameView): string {
  if (game.state === 'connected' && playing(held.value, game.id)) return t('games.state.playing')
  return t(`games.state.${game.state}`)
}

/** The effects offered during a match: those made for the game first. */
function effectsFor(game: string): EffectEntry[] {
  const mine = effects.value.filter((e) => e.game === game)
  return [...mine, ...effects.value.filter((e) => e.game !== game)]
}

/** Saves the game's *while playing*, or removes it with `null`. */
function whilePlaying(game: GameView, wanted: { effect: string; devices: DeviceInfo[] } | null) {
  return act(async () => {
    const next = withGameRule(
      rules.value,
      game.id,
      t('games.ruleName', { game: game.name }),
      wanted && {
        effect: wanted.effect,
        devices: wanted.devices.map((d) => ({ vid: d.vid, pid: d.pid })),
      },
    )
    await setRules(next)
    rules.value = next
  })
}

function toggleWhilePlaying(game: GameView, on: boolean): Promise<void> {
  if (!on) return whilePlaying(game, null)
  const effect = effectsFor(game.id)[0]?.id
  if (!effect) return Promise.resolve()
  return whilePlaying(game, { effect, devices: controlled.value })
}

function ruleDevices(rule: Rule): DeviceInfo[] {
  return controlled.value.filter((d) => rule.devices.some((r) => r.vid === d.vid && r.pid === d.pid))
}

function setEffect(game: GameView, rule: Rule, effect: string): Promise<void> {
  return whilePlaying(game, { effect, devices: ruleDevices(rule) })
}

function setDevice(game: GameView, rule: Rule, device: DeviceInfo, on: boolean): Promise<void> {
  const chosen = ruleDevices(rule).filter((d) => ids(d) !== ids(device))
  return whilePlaying(game, { effect: rule.show.effect, devices: on ? [...chosen, device] : chosen })
}

onMounted(async () => {
  await load()
  unlisten = await onSignalsChanged(() => void readSignals())
})
onBeforeUnmount(() => unlisten?.())
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>{{ t('games.title') }}</h1>
      <p class="lead">{{ t('games.lead') }}</p>
    </header>

    <article v-for="game in games" :key="game.id" class="card">
      <div class="top">
        <h2>{{ game.name }}</h2>
        <span class="state" :class="[game.state, { live: playing(held, game.id) }]">
          {{ stateOf(game) }}
        </span>
        <span class="spacer" />
        <button
          v-if="game.state === 'disconnected' || game.state === 'outdated'"
          type="button"
          class="solid"
          :disabled="busy"
          @click="connect(game)"
        >
          {{ game.state === 'outdated' ? t('games.reconnect') : t('games.connect') }}
        </button>
        <button
          v-if="game.state === 'connected' || game.state === 'outdated'"
          type="button"
          class="ghost"
          :disabled="busy"
          @click="disconnect(game)"
        >
          {{ t('games.disconnect') }}
        </button>
      </div>
      <p v-if="restart === game.name" class="note" role="status">
        {{ t('games.restart', { game: game.name }) }}
      </p>

      <template v-if="game.state !== 'notFound'">
        <!-- What it says now: the simplest way to see it works. -->
        <h3>{{ t('games.values') }}</h3>
        <dl v-if="gameSignals(held, game.id).length" class="values">
          <template v-for="s in gameSignals(held, game.id)" :key="s.name">
            <dt class="mono">{{ s.name }}</dt>
            <dd class="mono">{{ signalText(s.value) }}</dd>
          </template>
        </dl>
        <p v-else class="note">{{ t('games.quiet') }}</p>

        <h3>
          <label class="check">
            <input
              type="checkbox"
              :checked="gameRule(rules, game.id) !== undefined"
              :disabled="busy || controlled.length === 0"
              @change="toggleWhilePlaying(game, ($event.target as HTMLInputElement).checked)"
            />
            {{ t('games.whilePlaying') }}
          </label>
        </h3>
        <p v-if="controlled.length === 0" class="note">{{ t('games.noDevice') }}</p>
        <div v-else-if="gameRule(rules, game.id)" class="playing">
          <label>
            {{ t('games.show') }}
            <select
              :value="gameRule(rules, game.id)!.show.effect"
              :disabled="busy"
              @change="setEffect(game, gameRule(rules, game.id)!, ($event.target as HTMLSelectElement).value)"
            >
              <option v-for="e in effectsFor(game.id)" :key="e.id" :value="e.id">{{ e.name }}</option>
            </select>
          </label>
          <span>{{ t('games.on') }}</span>
          <label v-for="d in controlled" :key="ids(d)" class="check">
            <input
              type="checkbox"
              :checked="ruleDevices(gameRule(rules, game.id)!).some((r) => ids(r) === ids(d))"
              :disabled="busy"
              @change="setDevice(game, gameRule(rules, game.id)!, d, ($event.target as HTMLInputElement).checked)"
            />
            {{ d.name }}
          </label>
          <p class="note">{{ t('games.ruleNote') }}</p>
        </div>
      </template>
    </article>

    <FailureNote v-if="problem" @close="problem = null">{{ problem }}</FailureNote>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  padding: var(--gap-4);
  max-width: 820px;
}

.head h1 {
  margin: 0;
}

.lead {
  margin: var(--gap-1) 0 0;
  color: var(--text-muted);
}

.card {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-3) var(--gap-4);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.top {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-3);
}

.top h2 {
  margin: 0;
  font-size: 16px;
}

h3 {
  margin: var(--gap-2) 0 0;
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.spacer {
  flex: 1;
}

.state {
  color: var(--text-faint);
  font-size: 12px;
}

.state.connected {
  color: var(--ok);
}

.state.outdated {
  color: var(--warn);
}

.state.live {
  color: var(--accent);
  font-weight: 500;
}

.values {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 2px var(--gap-3);
  margin: 0;
  font-size: 12px;
}

.values dd {
  margin: 0;
}

.mono {
  font-family: var(--font-mono);
}

.playing {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-2) var(--gap-3);
  font-size: 13px;
}

.playing select {
  margin-left: var(--gap-2);
  padding: 4px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font: inherit;
}

.playing .note {
  flex-basis: 100%;
}

.check {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-2);
  text-transform: none;
  letter-spacing: normal;
  font-size: 13px;
  color: var(--text);
}

.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
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
</style>
