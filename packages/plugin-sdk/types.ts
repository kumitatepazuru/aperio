import type { ItemStructure } from "native";
import type { ReactNode } from "react";

/**
 * プラグインへ渡される args の型。
 */
export interface PluginComponentArgsMap {
  item: {
    item: ItemStructure;
    left: number;
    width: number;
    top: number;
    isSelected: boolean;
    /** このアイテム自身がドラッグされている間だけ true */
    isDragging: boolean;
    layerHeight: number;
  };
}

export type PluginComponentType = keyof PluginComponentArgsMap;

/**
 * プラグインの `index.js` が default export するコンポーネントの props。
 */
export interface PluginComponentProps<T extends PluginComponentType> {
  args: PluginComponentArgsMap[T];
  children: ReactNode;
}
