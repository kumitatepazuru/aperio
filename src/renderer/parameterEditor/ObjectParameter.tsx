import type { ConfigableValue } from "@/configable/utils";
import Configable from "@/configable/Configable";
import { initValues } from "@/configable/utils";
import useStore, { getStoreState } from "@shared/store";
import type { GeneratorInformation, ItemStructure } from "native";
import { useEffect, useState } from "react";
import { hasSameItems } from "@shared/utils/hasSame";
import { resolveGroupMoveDelta } from "@shared/utils/layerUtils";

const ObjectParameter = () => {
  const [structures, setStructures] = useState<
    GeneratorInformation["structure"]
  >([]);
  const [params, setParams] = useState<Record<string, ConfigableValue>>({});

  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const selectedItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === state.mainSelectedItemId),
  );

  /**
   * struct の minFrame / maxFrame を item に適用し、必要に応じて start / end を調整した
   * 新しい ItemStructure を返す。変更がなければ同一オブジェクトを返す。
   *
   * maxFrame 超過 → end を切り詰める。
   * minFrame 未満 → resolveGroupMoveDelta で衝突を避けながら start / end を移動させて伸ばす。
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

    let newStart = item.start;
    let newEnd = item.end;

    if (newMax !== undefined && newEnd - newStart > newMax) {
      newEnd = newStart + newMax;
    }

    const maxConstraint = newMax ?? Infinity;
    if (
      newMin !== undefined &&
      newMin <= maxConstraint &&
      newEnd - newStart < newMin
    ) {
      // minFrame に合わせた仮アイテムで resolveGroupMoveDelta を呼び、
      // 衝突しない最近傍の位置（delta）を求める。
      const initForMin = {
        id: item.id,
        start: newStart,
        end: newStart + newMin,
        layer: item.layer,
      };
      const movingIds = new Set([item.id]);
      const delta = resolveGroupMoveDelta(
        timeline,
        movingIds,
        [initForMin],
        0,
        0,
      );
      newStart = Math.max(0, newStart + delta);
      newEnd = newStart + newMin;
    }

    return { ...item, min: newMin, max: newMax, start: newStart, end: newEnd };
  };

  const updateTimeline = async (
    struct: GeneratorInformation,
    params: Record<string, ConfigableValue>,
  ) => {
    const timeline = (await getStoreState()).timelineItems;
    setTimelineItems(
      timeline.map((item): ItemStructure => {
        if (item.id !== selectedItemId) return item;
        const bounded = applyBounds(item, struct, timeline);
        return {
          ...bounded,
          object: { ...bounded.object, parameters: params },
        };
      }),
    );
  };

  useEffect(() => {
    if (!selectedItem) {
      setStructures([]);
      setParams({});
      return;
    }
    window.main
      .requestParameterStruct(
        selectedItem.object.name,
        selectedItem.object.parameters,
      )
      .then(async (struct) => {
        // TODO: 外部ファイルを参照している場合、保存→ファイル変更→読み込みをすると内部データの更新をする必要があるためupdateTimelineをここでも走らせている
        // TODO: ただ、プロジェクト読み込み時にすべてのupdateをして将来的にこれは削除するべき
        setStructures(struct.structure);
        const newParams = initValues(
          struct.structure,
          selectedItem.object.parameters,
        );
        setParams(newParams);
        void updateTimeline(struct, newParams);
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
        paramsToSave = {
          ...initValues(struct.structure, newParams),
          ...newParams,
        };
        setParams(paramsToSave);
      }

      // timelineのパラメータ情報を更新
      await updateTimeline(struct, paramsToSave);
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
