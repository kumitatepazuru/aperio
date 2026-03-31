import useStore, { getStoreState } from "@shared/store";
import type { GenerateStructure } from "native";
import { IoIosAdd } from "react-icons/io";
import NestedMenu, { type NestedMenuItems } from "@shared/Menu";
import BaseParameter from "./baseParameter";
import ObjectParameter from "./ObjectParameter";
import EffectParameter from "./EffectParameter";
import { v4 as uuidv4 } from "uuid";

const ParameterEditor = () => {
  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const onAddEffect = async (name: string) => {
    console.log("Add effect:", name);

    const structure = await window.main.requestNewEffectGenerator(name);
    const defaultParams: Record<string, unknown> = {};
    structure.structure.forEach((param) => {
      defaultParams[param.id] = param.defaultValue;
    });
    const timeline = (await getStoreState()).timelineLayers;
    const newEffect: GenerateStructure = {
      id: uuidv4(),
      name,
      displayName: structure.displayName,
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
    <div className="p-2 overflow-y-auto h-full">
      {selectedItem ? (
        <div>
          <div className="flex gap-3 items-center mb-4">
            <h2 className="text-lg font-bold grow">{selectedItem.obj.displayName}</h2>
            <NestedMenu click items={effectMenuItems}>
              <button className="btn btn-sm btn-square">
                <IoIosAdd />
              </button>
            </NestedMenu>
          </div>
          <div className="flex flex-col gap-3">
            <div className="divider divider-start text-sm my-0">ベース</div>
            <div className="card card-sm bg-base-300">
              <div className="card-body">
                <BaseParameter />
              </div>
            </div>
            <div className="divider divider-start text-sm my-0">
              オブジェクト
            </div>
            <div className="card card-sm bg-base-300">
              <div className="card-body">
                <ObjectParameter />
              </div>
            </div>
            <div className="divider divider-start text-sm my-0">エフェクト</div>
            <EffectParameter />
          </div>
        </div>
      ) : null}
    </div>
  );
};

export default ParameterEditor;
