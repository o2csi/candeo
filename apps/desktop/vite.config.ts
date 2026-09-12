import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [vue()],

  build: {
    // Monaco embarque le compilateur TypeScript : son ouvrier pese pres de
    // 7 Mo, et le compilateur charge a la validation 3,5 Mo de plus. Le seuil
    // par defaut (500 ko) avertirait donc a chaque construction d'un poids
    // choisi : l'application est empaquetee, rien n'est telecharge a l'usage,
    // et ces morceaux ne sont lus qu'a l'ouverture de l'editeur.
    // Voir `docs/design/studio.md` §2.
    chunkSizeWarningLimit: 8000,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
