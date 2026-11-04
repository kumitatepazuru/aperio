import type { Context } from "react";

/**
 * タイムラインの表示状態。TimelineBaseUI やその配下(LayerItems に差し込まれるプラグインを含む)から参照する。
 */
export type TimelineContextValue = {
  /** ZoomLevels のインデックス */
  zoom: number;
  /** ズームレベルごとの 1 フレームあたりのピクセル数 */
  zoomLevels: number[];
  /** 現在のズームでの 1 フレームあたりのピクセル数(zoomLevels[zoom]) */
  zoomLevelPxPerFrame: number;
  setZoom: (nextZoom: number) => void;
  horizontalScroll: number;
  maxVisibleLayers: number;
  layerHeight: number;
  contentWidth: number;
  graduationInterval: number;
  visibleGraduationIndices: number[];
  visibleVerticalLineIndices: number[];
};

export declare const TimelineContext: Context<TimelineContextValue | null>;

/** Timeline の内側(`item` などのプラグインコンポーネント)でのみ使用できる。 */
export declare function useTimeline(): TimelineContextValue;
