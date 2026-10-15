import { defineConfig, type Plugin } from "vite";
import { existsSync, mkdirSync, readdirSync, copyFileSync, createReadStream } from "node:fs";
import { resolve, join, extname } from "node:path";

// ───────────────────────────────────────────────────────────────────────────────
// THE one and only place the shared sound folder is declared.
// The 28 WAVs live in the macOS app and are NOT duplicated in the repo; when they
// move to `shared/sounds/`, change this single line.
export const SOUNDS_DIR = resolve(__dirname, "../NotchBuddy/Resources/sounds");
// ───────────────────────────────────────────────────────────────────────────────

/**
 * Serves SOUNDS_DIR at /sounds/*.wav in dev, and copies it into dist/sounds on build.
 * Keeps the WAVs out of windows/ while still shipping them inside the installer.
 */
function sharedSounds(): Plugin {
  const prefix = "/sounds/";
  return {
    name: "coucou-shared-sounds",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (!req.url?.startsWith(prefix)) return next();
        const name = decodeURIComponent(req.url.slice(prefix.length).split("?")[0]);
        if (name.includes("/") || name.includes("\\") || extname(name) !== ".wav") return next();
        const file = join(SOUNDS_DIR, name);
        if (!existsSync(file)) return next();
        res.setHeader("Content-Type", "audio/wav");
        createReadStream(file).pipe(res);
      });
    },
    closeBundle() {
      const out = resolve(__dirname, "dist/sounds");
      if (!existsSync(SOUNDS_DIR)) {
        this.warn(`sounds not found at ${SOUNDS_DIR} — the build will ship without audio`);
        return;
      }
      mkdirSync(out, { recursive: true });
      for (const f of readdirSync(SOUNDS_DIR)) {
        if (extname(f) === ".wav") copyFileSync(join(SOUNDS_DIR, f), join(out, f));
      }
    },
  };
}

// ───────────────────────────────────────────────────────────────────────────────
// Oma's expression pictures live in the top-level oma/ folder, shared by every app.
export const OMA_SPRITES_DIR = resolve(__dirname, "../oma/character/sprites");
// ───────────────────────────────────────────────────────────────────────────────

const OMA_TYPES: Record<string, string> = { ".png": "image/png", ".json": "application/json" };

/** Serves OMA_SPRITES_DIR at /oma/* in dev, and copies it into dist/oma on build. Absent pictures are fine:
 *  the app then draws Oma in code. */
function sharedOma(): Plugin {
  const prefix = "/oma/";
  return {
    name: "oma-shared-sprites",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (!req.url?.startsWith(prefix)) return next();
        const name = decodeURIComponent(req.url.slice(prefix.length).split("?")[0]);
        const type = OMA_TYPES[extname(name)];
        if (name.includes("/") || name.includes("\\") || !type) return next();
        const file = join(OMA_SPRITES_DIR, name);
        if (!existsSync(file)) return next();
        res.setHeader("Content-Type", type);
        createReadStream(file).pipe(res);
      });
    },
    closeBundle() {
      if (!existsSync(OMA_SPRITES_DIR)) return;
      const out = resolve(__dirname, "dist/oma");
      mkdirSync(out, { recursive: true });
      for (const f of readdirSync(OMA_SPRITES_DIR)) {
        if (OMA_TYPES[extname(f)]) copyFileSync(join(OMA_SPRITES_DIR, f), join(out, f));
      }
    },
  };
}

export default defineConfig({
  plugins: [sharedSounds(), sharedOma()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
    emptyOutDir: true,
    rollupOptions: {
      input: {
        island: resolve(__dirname, "index.html"),
        settings: resolve(__dirname, "settings.html"),
      },
    },
  },
});
