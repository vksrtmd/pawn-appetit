/// <reference types="vitest" />
import { resolve } from "node:path";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import { vanillaExtractPlugin } from "@vanilla-extract/vite-plugin";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const isDebug = !!process.env.TAURI_ENV_DEBUG;

const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [tanstackRouter(), react(), vanillaExtractPlugin()],
    clearScreen: false,
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
            ignored: ["**/src-tauri/**"],
        },
    },
    build: {
        minify: isDebug ? false : "esbuild",
        sourcemap: isDebug ? "inline" : false,
        rollupOptions: {
            output: {
                entryFileNames: "assets/[name].js",
                chunkFileNames: "assets/[name].js",
                assetFileNames: "assets/[name].[ext]",
            },
        },
    },
    resolve: {
        alias: [{ find: "@", replacement: resolve(__dirname, "./src") }],
    },
    test: {
        environment: "jsdom",
        env: { TZ: "UTC" },
        // Node 25+ ships a global localStorage that is undefined without
        // --localstorage-file and shadows jsdom's; turn it off in workers.
        execArgv: ["--no-experimental-webstorage"],
        // Every run also writes its results to disk, so a failure seen once
        // (a flaky test) keeps its name after the terminal output is gone.
        reporters: ["default", "json"],
        outputFile: { json: "node_modules/.vitest/last-run.json" },
    },
});
