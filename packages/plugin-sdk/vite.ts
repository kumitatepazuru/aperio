import react from "@vitejs/plugin-react";
import { createRequire } from "node:module";
import * as path from "node:path";
import type { Plugin } from "vite";

/** ホストが `globalThis` に公開している共有モジュール置き場の名前 (`apps/ui/plugin/shared.ts` と一致させる) */
const SHARED_GLOBAL = "__APERIO_SHARED__";
const VIRTUAL_PREFIX = "\0aperio-shared:";

/**
 * プラグイン側でバンドルせず、ホストのインスタンスを共有するモジュール。
 * react が二重に読み込まれると hooks が壊れるため、必ずホストのものを使う。
 */
const SHARED_MODULES = [
  "react",
  "react/jsx-runtime",
  "react-dom",
  "zustand",
  "zustand/shallow",
  "@aperio/plugin-sdk/store",
] as const;

/** 型だけの d.ts でビルド時に実体を読めないモジュールの named export */
const FIXED_EXPORTS: Record<string, string[]> = {
  "@aperio/plugin-sdk/store": ["useStore", "getStoreState"],
};

const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;

function sharedModules(): Plugin {
  let root = process.cwd();
  const exportNames = (id: string) => {
    const fixed = FIXED_EXPORTS[id];
    if (fixed) return fixed;
    const req = createRequire(path.join(root, "package.json"));
    return Object.keys(req(id)).filter(
      (k) => k !== "default" && k !== "__esModule" && IDENTIFIER.test(k),
    );
  };

  return {
    name: "aperio-plugin:shared-modules",
    enforce: "pre",
    configResolved(config) {
      root = config.root;
    },
    resolveId(source) {
      if ((SHARED_MODULES as readonly string[]).includes(source)) {
        return VIRTUAL_PREFIX + source;
      }
    },
    load(id) {
      if (!id.startsWith(VIRTUAL_PREFIX)) return;
      const source = id.slice(VIRTUAL_PREFIX.length);
      const names = exportNames(source);
      return [
        `const m = globalThis[${JSON.stringify(SHARED_GLOBAL)}]?.[${JSON.stringify(source)}];`,
        `if (!m) throw new Error(${JSON.stringify(`[aperio] shared module '${source}' is not provided by the host`)});`,
        `export default m.default ?? m;`,
        names.length > 0 ? `export const { ${names.join(", ")} } = m;` : "",
      ].join("\n");
    },
  };
}

/**
 * プラグインは html を持たず `index.js` 1 ファイルだけを配信するので、
 * 出力された css は js の先頭で <style> として挿入する形にまとめる。
 */
function inlineCss(): Plugin {
  return {
    name: "aperio-plugin:inline-css",
    apply: "build",
    enforce: "post",
    generateBundle(_, bundle) {
      const css: string[] = [];
      for (const [name, file] of Object.entries(bundle)) {
        if (file.type === "asset" && name.endsWith(".css")) {
          css.push(String(file.source));
          delete bundle[name];
        }
      }
      if (css.length === 0) return;

      const entry = Object.values(bundle).find(
        (file) => file.type === "chunk" && file.isEntry,
      );
      if (entry?.type !== "chunk") return;
      entry.code =
        `(()=>{const s=document.createElement("style");` +
        `s.textContent=${JSON.stringify(css.join("\n"))};document.head.appendChild(s)})();\n` +
        entry.code;
    },
  };
}

export interface AperioPluginOptions {
  /** プラグインのエントリ。default export に React コンポーネントを置く。 */
  entry?: string;
}

/**
 * Aperio の React プラグイン用 vite プリセット。
 * html を使わず `base: "./"` の `index.js` を 1 つ出力し、react / zustand / ホストの store はホスト側のものを共有する。
 */
export function aperioPlugin(options: AperioPluginOptions = {}): Plugin[] {
  const { entry = "src/index.tsx" } = options;
  return [
    sharedModules(),
    ...react(),
    {
      name: "aperio-plugin:config",
      config: () => ({
        base: "./",
        define: { "process.env.NODE_ENV": JSON.stringify("production") },
        build: {
          lib: { entry, formats: ["es"] },
          emptyOutDir: true,
          rolldownOptions: {
            output: { entryFileNames: "index.js" },
          },
        },
      }),
    },
    inlineCss(),
  ];
}
