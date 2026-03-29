import useStore, { getStoreState } from "@shared/store";
import type { GenerateStructure } from "native";
import { IoIosAdd } from "react-icons/io";
import NestedMenu, { type NestedMenuItems } from "@shared/Menu";
import BaseParameter from "./baseParameter";
import ObjectParameter from "./ObjectParameter";
import EffectParameter from "./EffectParameter";

const ParameterEditor = () => {
  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const onAddEffect = async (id: string) => {
    console.log("Add effect with ID:", id);

    const structure = await window.main.requestNewEffectGenerator(id);
    const defaultParams: Record<string, unknown> = {};
    structure.structure.forEach((param) => {
      defaultParams[param.id] = param.defaultValue;
    });
    const timeline = (await getStoreState()).timelineLayers;
    const newEffect: GenerateStructure = {
      name: id,
      parameters: defaultParams,
    };

    setTimelineLayers(
      timeline.map((layer) =>
        layer.id === selectedItemId
          ? {
              ...layer,
              effects: [...layer.effects, newEffect],
            }
          : layer,
      ),
    );

    return true; // メニューを閉じる
  };

  const effectMenuItems: () => Promise<NestedMenuItems> = async () => {
    const pluginNames = await window.main.getPluginNames();

    return Object.entries(pluginNames.basePlugin).map(([id, value]) => ({
      id,
      type: "submenu",
      value,
      submenu: Object.entries(pluginNames.effectPlugins).map(
        ([effectId, effectValue]) => ({
          id: effectId,
          type: "item",
          value: effectValue,
          click: onAddEffect,
        }),
      ),
    }));
  };

  return (
    <div className="p-2">
      {selectedItem ? (
        <div>
          <div className="flex gap-3 items-center">
            <h2 className="text-lg font-bold mb-2 grow">
              {selectedItem.obj.name}
            </h2>
            <NestedMenu click items={effectMenuItems}>
              <button className="btn btn-sm btn-square">
                <IoIosAdd />
              </button>
            </NestedMenu>
          </div>
          <div className="flex flex-col gap-3">
            <BaseParameter />
            <ObjectParameter />
            <EffectParameter />
          </div>
        </div>
      ) : null}
    </div>
  );
};

export default ParameterEditor;
