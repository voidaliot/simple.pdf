import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readFileSync } from "node:fs";

const tauriConfig = JSON.parse(readFileSync(new URL("../crates/app/tauri.conf.json", import.meta.url), "utf8"));

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  preview: { headers: { "Content-Security-Policy": tauriConfig.app.security.csp } },
  server: {
    port: 5173,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/crates/**", "**/target/**"] },
  },
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 1024,
  },
});
