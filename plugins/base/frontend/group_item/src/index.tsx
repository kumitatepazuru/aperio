import type { PluginComponentProps } from "@aperio/plugin-sdk/types";
import "./index.css";

const GROUP_OBJECT_NAME = "base_object.group_object";

export default function GroupItem({
  args,
  children,
}: PluginComponentProps<"item">) {
  const { item, left, width, top, layerHeight } = args;
  if (item.object.name !== GROUP_OBJECT_NAME) return <>{children}</>;

  // 0 = 一番下まで
  const layers = Number(item.object.parameters.layers ?? 0);

  return (
    <>
      {children}
      <div
        className="absolute z-20 bg-black/50 pointer-events-none"
        style={
          layers === 0
            ? { left, width, top: top + layerHeight, bottom: 0 }
            : { left, width, top: top + layerHeight, height: layerHeight * layers }
        }
      />
    </>
  );
}
