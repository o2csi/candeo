<script setup lang="ts">
/**
 * Settings › Games (`docs/design/game-state-integration.md` §4): the games
 * whose state Candeo can receive, and connecting one — writing the file the
 * game reads when it starts, and turning reception on if it was off.
 *
 * Its root is a `.block` of the Settings page, which gives it its place and its
 * separator; the rest is styled here.
 */
import { onMounted, ref, useId } from 'vue'

import { connectGame, disconnectGame, listGames, type GameView } from '../api/candeo'
import { message } from '../api/journal'
import { t } from '../i18n'
import FailureNote from './FailureNote.vue'

/** Connecting may turn reception on: the signals block reads it again. */
const emit = defineEmits<{ changed: [] }>()

const uid = useId()
const games = ref<GameView[]>([])
const busy = ref(false)
const problem = ref<string | null>(null)
/** The game just connected, which reads its file when it starts. */
const restart = ref<string | null>(null)

async function load(): Promise<void> {
  try {
    games.value = await listGames()
  } catch (e) {
    problem.value = message(e)
  }
}

async function act(game: GameView, connect: boolean): Promise<void> {
  busy.value = true
  problem.value = null
  restart.value = null
  try {
    games.value = connect ? await connectGame(game.id) : await disconnectGame(game.id)
    if (connect) restart.value = game.name
    emit('changed')
  } catch (e) {
    problem.value = message(e)
  } finally {
    busy.value = false
  }
}

onMounted(load)
</script>

<template>
  <section class="block" :aria-labelledby="`${uid}-title`">
    <h2 :id="`${uid}-title`">{{ t('settings.games.title') }}</h2>
    <p class="note">{{ t('settings.games.detail') }}</p>

    <ul class="games">
      <li v-for="game in games" :key="game.id">
        <span class="name">{{ game.name }}</span>
        <span class="state" :class="game.state">{{ t(`settings.games.state.${game.state}`) }}</span>
        <span class="actions">
          <button
            v-if="game.state === 'disconnected' || game.state === 'outdated'"
            type="button"
            class="solid"
            :disabled="busy"
            @click="act(game, true)"
          >
            {{ game.state === 'outdated' ? t('settings.games.reconnect') : t('settings.games.connect') }}
          </button>
          <button
            v-if="game.state === 'connected' || game.state === 'outdated'"
            type="button"
            class="ghost"
            :disabled="busy"
            @click="act(game, false)"
          >
            {{ t('settings.games.disconnect') }}
          </button>
        </span>
      </li>
    </ul>

    <p v-if="restart" class="note" role="status">{{ t('settings.games.restart', { game: restart }) }}</p>
    <FailureNote v-if="problem" @close="problem = null">{{ problem }}</FailureNote>
  </section>
</template>

<style scoped>
.block {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--gap-2);
}

.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

.games {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.games li {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-3);
}

.name {
  font-weight: 500;
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

.actions {
  display: flex;
  gap: var(--gap-2);
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
