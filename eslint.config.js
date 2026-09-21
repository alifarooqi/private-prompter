/**
 * ESLint flat-config for the React + TypeScript frontend.
 *
 * We keep this deliberately minimal — the goal is to catch the common
 * accidental issues (unused vars, missing return types, etc.) without
 * fighting the codebase over style. Tighten rules as we go.
 */
import js from "@eslint/js";
import tsParser from "@typescript-eslint/parser";
import tsPlugin from "@typescript-eslint/eslint-plugin";

export default [
  js.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}", "*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
      globals: {
        window: "readonly",
        document: "readonly",
        console: "readonly",
        // Tauri invoke() invocations register their target as a free
        // function at the call site; these are common ones.
        invoke: "readonly",
        fetch: "readonly",
        URL: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
      },
    },
    plugins: {
      "@typescript-eslint": tsPlugin,
    },
    rules: {
      // The default rule set is too noisy on a Tauri app: Tauri-generated
      // types lean on empty interfaces, untyped imports, etc. Start with
      // the strict-but-tolerable subset and tighten as we go.
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "no-undef": "off", // TypeScript handles this better than ESLint.
      "no-empty": ["warn", { allowEmptyCatch: true }],
    },
  },
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      "src-tauri/target/**",
    ],
  },
];
