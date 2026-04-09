import Configable from "@/configable/Configable";
import type { ConfigableValue } from "@/configable/utils";
import { initValues } from "@/configable/utils";
import useStore, {
  getStoreState,
  type TimelineItemStructure,
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
  selectedItem: TimelineItemStructure | undefined;
  onParamsChange: (
    index: number,
    params: Record<string, ConfigableValue>,
  ) => void;
}

const SortableEffectItem = ({
  effect,
  index,
  selectedItemId,
  selectedItem,
  onParamsChange,
}: SortableEffectItemProps) => {
  const { ref } = useSortable({
    id: `${effect.id}`,
    index,
    group: selectedItemId ?? undefined,
  });
  const currentItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === selectedItemId),
  );

  const [structures, setStructures] = useState<RequestStructureParameter[]>([]);
  const [params, setParams] = useState<Record<string, ConfigableValue>>({});

  useEffect(() => {
    const currentParams = selectedItem?.effects[index]?.parameters ?? {};
    window.main
      .requestParameterStruct(effect.name, currentParams)
      .then((struct) => {
        setStructures(struct);
        setParams(initValues(struct, currentParams));
      })
      .catch(console.error);
    // selectedItemId または effect.name が変わったときだけ初期化する
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId, effect.name]);

  const handleChange = async (newParams: Record<string, ConfigableValue>) => {
    setParams(newParams);
    try {
      const struct = await window.main.requestParameterStruct(
        effect.name,
        newParams,
      );
      if (
        hasSameItems(
          struct.map((p) => p.id),
          structures.map((p) => p.id),
        )
      ) {
        onParamsChange(index, newParams);
      } else {
        setStructures(struct);
        // 追加のパラメータを初期化する
        const newParamsWithInit = {
          ...initValues(struct, newParams),
          ...newParams,
        };
        setParams(newParamsWithInit);
        await onParamsChange(index, newParamsWithInit);
      }
    } catch (error) {
      console.error(error);
    }
  };

  const changeOrder = async (amount: number) => {
    if (!selectedItemId || !currentItem) return;

    const { timelineItems: timeline, setTimelineItems } =
      await getStoreState();

    const currentIndex = currentItem.effects.findIndex(
      (e) => e.id === effect.id,
    );
    if (currentIndex === -1) return;

    const newIndex = currentIndex + amount;
    if (newIndex < 0 || newIndex >= currentItem.effects.length) return;

    const newEffects = arrayMove(currentItem.effects, currentIndex, newIndex);
    const newTimeline = timeline.map((item) =>
      item.id === selectedItemId ? { ...item, effects: newEffects } : item,
    );
    setTimelineItems(newTimeline);
  };

  const deleteEffect = async () => {
    if (!selectedItemId || !currentItem) return;

    const { timelineItems: timeline, setTimelineItems } =
      await getStoreState();

    const newEffects = currentItem.effects.filter((e) => e.id !== effect.id);
    const newTimeline = timeline.map((item) =>
      item.id === selectedItemId ? { ...item, effects: newEffects } : item,
    );
    setTimelineItems(newTimeline);
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
                currentItem?.effects.length
                  ? currentItem.effects.length - 1 - index
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
          value={params}
          onChange={handleChange}
        />
      </div>
    </div>
  );
};

const EffectParameter = () => {
  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const selectedItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === state.mainSelectedItemId),
  );

  const handleParamsChange = async (
    index: number,
    params: Record<string, ConfigableValue>,
  ) => {
    if (!selectedItemId || !selectedItem) return;
    const timeline = (await getStoreState()).timelineItems;
    setTimelineItems(
      timeline.map((item) =>
        item.id === selectedItemId
          ? {
              ...item,
              effects: item.effects.map((effect, i) =>
                i === index ? { ...effect, parameters: params } : effect,
              ),
            }
          : item,
      ),
    );
  };

  const handleDragEnd: DragEndEvent = async ({ operation }) => {
    if (!isSortableOperation(operation)) return;
    if (!selectedItemId) return;

    const { source, target } = operation;
    if (!source) return;
    if (!target || source.index === target.index) return;

    const timeline = (await getStoreState()).timelineItems;
    setTimelineItems(
      timeline.map((item) =>
        item.id === selectedItemId
          ? {
              ...item,
              effects: arrayMove(item.effects, source.index, target.index),
            }
          : item,
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
          onParamsChange={handleParamsChange}
        />
      ))}
    </DragDropProvider>
  );
};

export default EffectParameter;
