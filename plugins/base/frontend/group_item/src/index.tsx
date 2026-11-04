import type { PluginComponentProps } from "@aperio/plugin-sdk/types";
import { useTimeline } from "@aperio/plugin-sdk/timeline";
import "./index.css";

const GROUP_OBJECT_NAME = "base_object.group_object";

export default function GroupItem({
  args,
  children,
}: PluginComponentProps<"item">) {
  const { item, left, width, top, layerHeight } = args;
  const { maxVisibleLayers } = useTimeline();
  if (item.object.name !== GROUP_OBJECT_NAME) return <>{children}</>;

  // 0 = 一番下まで
  const layers = Math.min(
    Number(item.object.parameters.layers ?? 0),
    maxVisibleLayers - item.layer - 1,
  );

  return (
    <>
      {children}
      <div
        className="absolute z-20 bg-black/40 pointer-events-none"
        style={
          layers === 0
            ? { left, width, top: top + layerHeight, bottom: 0 }
            : {
                left,
                width,
                top: top + layerHeight,
                height: layerHeight * layers,
              }
        }
      />
    </>
  );
}
