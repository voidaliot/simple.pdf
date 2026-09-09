import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

const tauriConfig = JSON.parse(readFileSync(new URL("../crates/app/tauri.conf.json", import.meta.url), "utf8"));

function plantumlAssets(): Plugin {
  // TeaVM loads these optional built-in packs by fixed relative names. They
  // must be available beside plantuml.html, including in packaged Tauri builds.
  const assets = ["openiconic.js", "emoji.js"].map((name) => ({
    name, source: readFileSync(new URL(`./node_modules/@plantuml/core/${name}`, import.meta.url)),
  }));
  let building = false;
  return {
    name: "plantuml-builtin-assets",
    configResolved(config) { building = config.command === "build"; },
    buildStart() {
      if (!building) return;
      for (const asset of assets) this.emitFile({ type: "asset", fileName: asset.name, source: asset.source });
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const asset = assets.find((asset) => request.url?.split("?")[0] === `/${asset.name}`);
        if (!asset) return next();
        response.setHeader("Content-Type", "text/javascript; charset=utf-8");
        response.end(asset.source);
      });
    },
  };
}

export default defineConfig({
  plugins: [svelte(), plantumlAssets()],
  clearScreen: false,
  preview: { headers: { "Content-Security-Policy": tauriConfig.app.security.csp } },
  server: {
    port: 5173,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/crates/**", "**/target/**"] },
  },
  build: {
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        plantuml: fileURLToPath(new URL("./plantuml.html", import.meta.url)),
      },
    },
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 1024,
  },
});
