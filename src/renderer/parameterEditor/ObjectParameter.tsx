import type { ConfigableValue } from "@/configable/utils";
import Configable from "@/configable/Configable";
import { initValues } from "@/configable/utils";
import useStore, { getStoreState } from "@shared/store";
import type { GeneratorInformation, ItemStructure } from "native";
import { useEffect, useState } from "react";
import { hasSameItems } from "@shared/utils/hasSame";
import { resolveGroupMoveDelta } from "@shared/utils/layerUtils";

const ObjectParameter = () => {
  const [structures, setStructures] = useState<GeneratorInformation["structure"]>([]);
  const [params, setParams] = useState<Record<string, ConfigableValue>>({});

  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const selectedItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === state.mainSelectedItemId),
  );

  /**
   * struct の minFrame / maxFrame を item に適用し、必要に応じて from / to を調整した
   * 新しい ItemStructure を返す。変更がなければ同一オブジェクトを返す。
   *
   * maxFrame 超過 → to を切り詰める。
   * minFrame 未満 → resolveGroupMoveDelta で衝突を避けながら from / to を移動させて伸ばす。
   * maxFrame < minFrame の矛盾がある場合は max 優先で min の調整をスキップする。
   */
  const applyBounds = (
    item: ItemStructure,
    struct: GeneratorInformation,
    timeline: ItemStructure[],
  ): ItemStructure => {
    const newMin = struct.minFrame;
    const newMax = struct.maxFrame;
    if (newMin === item.min && newMax === item.max) return item;

    let newFrom = item.from;
    let newTo = item.to;

    if (newMax !== undefined && newTo - newFrom > newMax) {
      newTo = newFrom + newMax;
    }

    const maxConstraint = newMax ?? Infinity;
    if (newMin !== undefined && newMin <= maxConstraint && newTo - newFrom < newMin) {
      // minFrame に合わせた仮アイテムで resolveGroupMoveDelta を呼び、
      // 衝突しない最近傍の位置（delta）を求める。
      const initForMin = { id: item.id, from: newFrom, to: newFrom + newMin, layer: item.layer };
      const movingIds = new Set([item.id]);
      const delta = resolveGroupMoveDelta(timeline, movingIds, [initForMin], 0, 0);
      newFrom = Math.max(0, newFrom + delta);
      newTo = newFrom + newMin;
    }

    return { ...item, min: newMin, max: newMax, from: newFrom, to: newTo };
  };

  const applyStructBounds = async (
    struct: GeneratorInformation,
    itemId: string,
  ) => {
    const timeline = (await getStoreState()).timelineItems;
    const item = timeline.find((i) => i.id === itemId);
    if (!item) return;

    const updated = applyBounds(item, struct, timeline);
    if (updated === item) return;

    setTimelineItems(
      timeline.map((i): ItemStructure => (i.id === itemId ? updated : i)),
    );
  };

  useEffect(() => {
    if (!selectedItem) {
      setStructures([]);
      setParams({});
      return;
    }
    const itemId = selectedItem.id;
    window.main
      .requestParameterStruct(
        selectedItem.object.name,
        selectedItem.object.parameters,
      )
      .then((struct) => {
        setStructures(struct.structure);
        setParams(initValues(struct.structure, selectedItem.object.parameters));
        void applyStructBounds(struct, itemId);
      })
      .catch(console.error);
    // selectedItemId が変わったときだけ初期化する
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId]);

  const handleChange = async (newParams: Record<string, ConfigableValue>) => {
    if (!selectedItemId || !selectedItem) return;
    setParams(newParams);

    try {
      const struct = await window.main.requestParameterStruct(
        selectedItem.object.name,
        newParams,
      );

      let paramsToSave = newParams;
      if (
        !hasSameItems(
          struct.structure.map((p) => p.id),
          structures.map((p) => p.id),
        )
      ) {
        setStructures(struct.structure);
        paramsToSave = { ...initValues(struct.structure, newParams), ...newParams };
        setParams(paramsToSave);
      }

      const timeline = (await getStoreState()).timelineItems;
      setTimelineItems(
        timeline.map((item): ItemStructure => {
          if (item.id !== selectedItemId) return item;
          const bounded = applyBounds(item, struct, timeline);
          return {
            ...bounded,
            object: { ...bounded.object, parameters: paramsToSave },
          };
        }),
      );
    } catch (error) {
      console.error("Error fetching parameter structure:", error);
    }
  };

  return (
    <Configable
      structures={structures}
      value={params}
      onChange={handleChange}
    />
  );
};

export default ObjectParameter;
