import { defineConfig } from "vitest/config";

// Scope vitest to the React app under src/. Without this, vitest walks
// into .build/llama.cpp/tools/ui/ (created by scripts/build-sidecar.sh)
// and tries to load its SvelteKit tests, which fail with
// "Cannot find module './.svelte-kit/tsconfig.json'".
export default defineConfig({
  test: {
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    exclude: ["node_modules", "dist", ".build", "src-tauri"],
  },
});
