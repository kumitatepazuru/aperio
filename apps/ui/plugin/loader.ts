import { getStoreState } from "@aperio/ui/store";
import type {
  PluginComponentProps,
  PluginComponentType,
} from "@aperio/plugin-sdk/types";
import type { ComponentType } from "react";

type LoadedComponent = ComponentType<PluginComponentProps<PluginComponentType>>;

const loaded = new Map<string, LoadedComponent>();

const key = (pluginId: string, type: string) => `${pluginId}/${type}`;

/** 読み込み済みのプラグインコンポーネントを返す。未登録・読込失敗なら undefined。 */
export function getPluginComponent(
  pluginId: string,
  type: string,
): LoadedComponent | undefined {
  return loaded.get(key(pluginId, type));
}

/**
 * store に登録されているプラグインコンポーネントを全て読み込む。
 * 描画を同期的に行えるよう、最初のレンダリングより前に呼ぶ。
 * 1 つのプラグインの失敗で他を巻き込まないよう、失敗した分はログだけ出してスキップする。
 */
export async function preloadPluginComponents(): Promise<void> {
  const { pluginComponents } = await getStoreState();
  await Promise.all(
    pluginComponents.map(async ({ pluginId, componentType }) => {
      const url = `plugin://${pluginId}/${componentType}/index.js`;
      try {
        const module = await import(/* @vite-ignore */ url);
        loaded.set(key(pluginId, componentType), module.default);
        console.info(`[plugin] loaded ${url}`);
      } catch (e) {
        console.error(`[plugin] failed to load ${url}`, e);
      }
    }),
  );
}
