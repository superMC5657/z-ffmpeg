import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

export default tseslint.config(
  {
    ignores: ["dist/**", "src-tauri/**", "node_modules/**", "scripts/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    plugins: { "react-hooks": reactHooks },
    languageOptions: {
      globals: { ...globals.browser },
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // 数据加载模式（effect 内先 setLoading(true) 再异步拉取）是该插件的
      // set-state-in-effect 规则的常见误报场景，降为警告保持可见但不阻塞 CI
      "react-hooks/set-state-in-effect": "warn",
      // 项目里大量"错误已就地处理"的空 catch 是有意为之（降级/回退），
      // 由 review 保证质量；这里只警告，避免噪音
      "no-empty": ["warn", { allowEmptyCatch: true }],
      "@typescript-eslint/no-explicit-any": "warn",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  }
);
