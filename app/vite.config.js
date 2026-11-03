import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const root = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  server: {
    host: "0.0.0.0",
    port: 5173,
    fs: { allow: [resolve(root, "..")] },
  },
});
