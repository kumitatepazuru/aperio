import type { PluginComponentProps } from "@aperio/plugin-sdk/types";
import "./index.css";

const GROUP_OBJECT_NAME = "base_object.group_object";

export default function GroupItem({
  args,
  children,
}: PluginComponentProps<"item">) {
  const { item, left, width, top, layerHeight } = args;
  if (item.object.name !== GROUP_OBJECT_NAME) return <>{children}</>;

  const layers = Number(item.object.parameters.layers ?? 1);

  return (
    <>
      {children}
      <div
        className="absolute z-20 bg-black/50 pointer-events-none"
        style={{ left, width, top: top + layerHeight, height: layerHeight * layers }}
      />
    </>
  );
}
