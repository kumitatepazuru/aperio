import Configable from "@/configable/Configable";
import type { ConfigableValue } from "@/configable/utils";
import { initValues } from "@/configable/utils";
import useStore, {
  getStoreState,
  type TimelineLayerStructure,
} from "@shared/store";
import type { GenerateStructure, RequestStructureParameter } from "native";
import { useEffect, useState } from "react";
import { DragDropProvider, type DragEndEvent } from "@dnd-kit/react";
import { isSortableOperation, useSortable } from "@dnd-kit/react/sortable";
import { FaAngleDown, FaAngleUp, FaXmark } from "react-icons/fa6";
import { FaAngleDoubleDown, FaAngleDoubleUp } from "react-icons/fa";
import { MdOutlineDragHandle } from "react-icons/md";
import { hasSameItems } from "@/utils/hasSame";

function arrayMove<T>(arr: T[], from: number, to: number): T[] {
  const result = [...arr];
  const [removed] = result.splice(from, 1);
  result.splice(to, 0, removed);
  return result;
}

interface SortableEffectItemProps {
  effect: GenerateStructure;
  index: number;
  selectedItemId: string | null;
  selectedItem: TimelineLayerStructure | undefined;
  onValuesChange: (
    index: number,
    values: Record<string, ConfigableValue>,
  ) => void;
}

const SortableEffectItem = ({
  effect,
  index,
  selectedItemId,
  selectedItem,
  onValuesChange,
}: SortableEffectItemProps) => {
  const { ref } = useSortable({
    id: `${effect.id}`,
    index,
    group: selectedItemId ?? undefined,
  });
  const currentLayer = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const [structures, setStructures] = useState<RequestStructureParameter[]>([]);
  const [values, setValues] = useState<Record<string, ConfigableValue>>({});

  useEffect(() => {
    const params = selectedItem?.effects[index]?.parameters ?? {};
    window.main
      .requestParameterStruct(effect.name, params)
      .then((struct) => {
        setStructures(struct);
        setValues(initValues(struct, params));
      })
      .catch(console.error);
    // selectedItemId または effect.name が変わったときだけ初期化する
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId, effect.name]);

  const handleChange = async (newValues: Record<string, ConfigableValue>) => {
    setValues(newValues);
    try {
      const struct = await window.main.requestParameterStruct(
        effect.name,
        newValues,
      );
      if (
        hasSameItems(
          struct.map((p) => p.id),
          structures.map((p) => p.id),
        )
      ) {
        onValuesChange(index, newValues);
      } else {
        setStructures(struct);
        // 追加のパラメータを初期化する
        setValues((prevValues) => ({
          ...initValues(struct, newValues),
          ...prevValues,
        }));
      }
    } catch (error) {
      console.error(error);
    }
  };

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
        <div className="absolute left-0 right-0 flex justify-center top-0 cursor-grab">
          <MdOutlineDragHandle size="1.5em" className="text-base-content/40" />
        </div>
        <h2 className="card-title">{effect.displayName}</h2>
        <div className="card-actions justify-end absolute right-0 pr-4 gap-0.5">
          <button
            className="btn btn-xs btn-circle"
            onClick={() => changeOrder(index * -1)}
          >
            <FaAngleDoubleUp />
          </button>
          <button
            className="btn btn-xs btn-circle"
            onClick={() => changeOrder(-1)}
          >
            <FaAngleUp />
          </button>
          <button
            className="btn btn-xs btn-circle"
            onClick={() => changeOrder(1)}
          >
            <FaAngleDown />
          </button>
          <button
            className="btn btn-xs btn-circle"
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
          <button className="btn btn-xs btn-circle" onClick={deleteEffect}>
            <FaXmark />
          </button>
        </div>
        <Configable
          structures={structures}
          value={values}
          onChange={handleChange}
        />
      </div>
    </div>
  );
};

const EffectParameter = () => {
  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === state.mainSelectedItemId),
  );

  const handleValuesChange = async (
    index: number,
    values: Record<string, ConfigableValue>,
  ) => {
    if (!selectedItemId || !selectedItem) return;
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
          selectedItemId={selectedItemId}
          selectedItem={selectedItem}
          onValuesChange={handleValuesChange}
        />
      ))}
    </DragDropProvider>
  );
};

export default EffectParameter;
