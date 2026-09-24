/// <reference types="vite/client" />

/** 在构建时从 package.json 的 `version` 注入（参见 vite.config.ts 中的 `define`） */
declare const __APP_VERSION__: string;
