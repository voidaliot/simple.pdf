import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
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
