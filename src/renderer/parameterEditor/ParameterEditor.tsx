import useStore, { getStoreState } from "@shared/store";
import type { GenerateStructure } from "native";
import { IoIosAdd } from "react-icons/io";
import NestedMenu, { type NestedMenuItems } from "@shared/Menu";
import BaseParameter from "./baseParameter";
import ObjectParameter from "./ObjectParameter";
import FilterParameter from "./FilterParameter";

const ParameterEditor = () => {
  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const onAddFilter = async (id: string) => {
    console.log("Add filter with ID:", id);

    const structure = await window.main.requestNewFilterGenerator(id);
    const defaultParams: Record<string, unknown> = {};
    structure.structure.forEach((param) => {
      defaultParams[param.id] = param.defaultValue;
    });
    const timeline = (await getStoreState()).timelineLayers;
    const newFilter: GenerateStructure = {
      name: id,
      parameters: defaultParams,
    };

    setTimelineLayers(
      timeline.map((layer) =>
        layer.id === selectedItemId
          ? {
              ...layer,
              effects: [...layer.effects, newFilter],
            }
          : layer,
      ),
    );

    return true; // メニューを閉じる
  };

  const filterMenuItems: () => Promise<NestedMenuItems> = async () => {
    const pluginNames = await window.main.getPluginNames();

    return Object.entries(pluginNames.basePlugin).map(([id, value]) => ({
      id,
      type: "submenu",
      value,
      submenu: Object.entries(pluginNames.filterPlugins).map(
        ([filterId, filterValue]) => ({
          id: filterId,
          type: "item",
          value: filterValue,
          click: onAddFilter,
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
            <NestedMenu click items={filterMenuItems}>
              <button className="btn btn-sm btn-square">
                <IoIosAdd />
              </button>
            </NestedMenu>
          </div>
          <div className="flex flex-col gap-3">
            <BaseParameter />
            <ObjectParameter />
            <FilterParameter />
          </div>
        </div>
      ) : null}
    </div>
  );
};

export default ParameterEditor;
