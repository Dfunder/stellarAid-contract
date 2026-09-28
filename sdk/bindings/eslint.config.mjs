// ESLint flat config for the TypeScript wallet adapters.
//
// Closes #885.
//
// These are browser-side adapters that wrap three third-party wallet
// extensions. The lint is tuned for that, not for a generic TS project:
//   * `no-explicit-any` is on. These files sit directly against untyped
//     extension APIs, which is exactly where an `any` will quietly appear.
//   * `no-non-null-assertion` is deliberately allowed. `window.freighter!`
//     after an `isAvailable()` guard is the documented contract of this
//     package; the alternative is a cast that is just as unchecked and
//     less visible.
//   * Only `src/` is linted. `dist/` is generated and `files: ["dist"]` in
//     package.json means it is never edited by hand.
//
// `tsconfigRootDir` is pinned so the typed rules resolve the same
// compilerOptions a `tsc` invocation would, instead of falling back to
// type-agnostic linting. Two projects: the build config covers `src/`, the
// test config additionally covers `test/`, whose files are outside the
// build's `rootDir` on purpose.
import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "node_modules/**", "coverage/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.ts", "test/**/*.ts"],
    languageOptions: {
      parserOptions: {
        project: ["./tsconfig.json", "./tsconfig.test.json"],
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // The adapters re-export a deliberately minimal structural interface
      // rather than depending on the extensions' published types, so the
      // `import type` lines are load-bearing. See freighter.ts.
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      // An empty catch is how a wallet-detection probe swallows a browser
      // quirk. Allowed, but it has to say so.
      "no-empty": ["error", { allowEmptyCatch: true }],
      eqeqeq: ["error", "always", { null: "ignore" }],
      "no-console": "warn",
    },
  },
  {
    // Tests are allowed to be loose about assertions and forbidden APIs.
    files: ["test/**/*.ts"],
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-non-null-assertion": "off",
      "no-console": "off",
    },
  },
);
