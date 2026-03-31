import Configable from "@/configable/Configable";
import type { ConfigableValue } from "@/configable/utils";
import useStore, {
  getStoreState,
  type TimelineLayerStructure,
} from "@shared/store";
import type { GenerateStructure, RequestStructureParameter } from "native";
import { useState } from "react";
import { DragDropProvider, type DragEndEvent } from "@dnd-kit/react";
import { isSortableOperation, useSortable } from "@dnd-kit/react/sortable";
import { FaAngleDown, FaAngleUp, FaXmark } from "react-icons/fa6";
import { FaAngleDoubleDown, FaAngleDoubleUp } from "react-icons/fa";

function arrayMove<T>(arr: T[], from: number, to: number): T[] {
  const result = [...arr];
  const [removed] = result.splice(from, 1);
  result.splice(to, 0, removed);
  return result;
}

interface SortableEffectItemProps {
  effect: GenerateStructure;
  index: number;
  structures: RequestStructureParameter[];
  tempStructures: Record<string, ConfigableValue> | null;
  selectedItemId: string | null;
  selectedItem: TimelineLayerStructure | undefined;
  onObjChange: (
    name: string,
    index: number,
    values: Record<string, ConfigableValue>,
  ) => void;
}

const SortableEffectItem = ({
  effect,
  index,
  structures,
  tempStructures,
  selectedItemId,
  selectedItem,
  onObjChange,
}: SortableEffectItemProps) => {
  const { ref } = useSortable({
    id: `${effect.id}`,
    index,
    group: selectedItemId ?? undefined,
  });
  const currentLayer = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const changeOrder = async (amount: number) => {
    if (!selectedItemId || !currentLayer) return;

    const { timelineLayers: timeline, setTimelineLayers } =
      await getStoreState();

    const currentIndex = currentLayer.effects.findIndex(
      (e) => e.id === effect.id,
    );
    if (currentIndex === -1) return;

    const newIndex = currentIndex + amount;
    if (newIndex < 0 || newIndex >= currentLayer.effects.length) return;

    const newEffects = arrayMove(currentLayer.effects, currentIndex, newIndex);
    const newTimeline = timeline.map((layer) =>
      layer.id === selectedItemId ? { ...layer, effects: newEffects } : layer,
    );
    setTimelineLayers(newTimeline);
  };

  const deleteEffect = async () => {
    if (!selectedItemId || !currentLayer) return;

    const { timelineLayers: timeline, setTimelineLayers } =
      await getStoreState();

    const newEffects = currentLayer.effects.filter((e) => e.id !== effect.id);
    const newTimeline = timeline.map((layer) =>
      layer.id === selectedItemId ? { ...layer, effects: newEffects } : layer,
    );
    setTimelineLayers(newTimeline);
  };

  return (
    <div ref={ref} className="card card-sm bg-base-300">
      <div className="card-body">
        <h2 className="card-title">{effect.displayName}</h2>
        <div className="card-actions justify-end">
          <button
            className="btn btn-sm btn-circle"
            onClick={() => changeOrder(index * -1)}
          >
            <FaAngleDoubleUp />
          </button>
          <button
            className="btn btn-sm btn-circle"
            onClick={() => changeOrder(-1)}
          >
            <FaAngleUp />
          </button>
          <button
            className="btn btn-sm btn-circle"
            onClick={() => changeOrder(1)}
          >
            <FaAngleDown />
          </button>
          <button
            className="btn btn-sm btn-circle"
            onClick={() =>
              changeOrder(
                currentLayer?.effects.length
                  ? currentLayer.effects.length - 1 - index
                  : 0,
              )
            }
          >
            <FaAngleDoubleDown />
          </button>
          <button className="btn btn-sm btn-circle" onClick={deleteEffect}>
            <FaXmark />
          </button>
        </div>
        <Configable
          key={`${effect.id}`}
          structures={structures || []}
          initialValues={
            tempStructures ?? selectedItem?.effects[index]?.parameters ?? {}
          }
          resetKey={selectedItemId ?? undefined}
          onChange={(values) => onObjChange(effect.name, index, values)}
          onInit={(values) => onObjChange(effect.name, index, values)}
        />
      </div>
    </div>
  );
};

const EffectParameter = () => {
  const [structures, setStructures] = useState<RequestStructureParameter[][]>(
    [],
  );
  const [tempStructures, setTempStructures] = useState<
    TimelineLayerStructure["obj"]["parameters"] | null
  >(null);

  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const handleObjChange = async (
    name: string,
    index: number,
    values: Record<string, ConfigableValue>,
  ) => {
    if (!selectedItemId || !selectedItem) return;

    (async () => {
      try {
        const struct = await window.main.requestParameterStruct(name, values);
        const structForCheck = struct.map((param) => param.id);
        const structuresIds = structures[index]?.map((param) => param.id) || [];
        // idがすべて一致するか確認
        if (
          structForCheck.every((id) => structuresIds.includes(id)) &&
          structForCheck.length === structuresIds.length
        ) {
          // 変更前と同じ構造なら更新
          const timeline = (await getStoreState()).timelineLayers;
          setTimelineLayers(
            timeline.map((layer) =>
              layer.id === selectedItemId
                ? {
                    ...layer,
                    effects: layer.effects.map((effect, i) =>
                      i === index ? { ...effect, parameters: values } : effect,
                    ),
                  }
                : layer,
            ),
          );
          setTempStructures(null);
        } else {
          // 変更前と構造が違うなら構造を更新してパラメーターは更新しない
          setStructures((prev) => ({
            ...prev,
            [index]: struct,
          }));
          setTempStructures(values);
        }
      } catch (error) {
        console.error("Error fetching parameter structure:", error);
      }
    })();
  };

  const handleDragEnd: DragEndEvent = async ({ operation }) => {
    if (!isSortableOperation(operation)) return;
    if (!selectedItemId) return;

    const { source, target } = operation;
    if (!source) return;
    if (!target || source.index === target.index) return;

    const timeline = (await getStoreState()).timelineLayers;
    setTimelineLayers(
      timeline.map((layer) =>
        layer.id === selectedItemId
          ? {
              ...layer,
              effects: arrayMove(layer.effects, source.index, target.index),
            }
          : layer,
      ),
    );
  };

  return (
    <DragDropProvider onDragEnd={handleDragEnd}>
      {selectedItem?.effects.map((effect, index) => (
        <SortableEffectItem
          key={`${effect.id}`}
          effect={effect}
          index={index}
          structures={structures[index] || []}
          tempStructures={tempStructures}
          selectedItemId={selectedItemId}
          selectedItem={selectedItem}
          onObjChange={handleObjChange}
        />
      ))}
    </DragDropProvider>
  );
};

export default EffectParameter;
