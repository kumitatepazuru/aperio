import type { ConfigableValue } from "@/configable/utils";
import Configable from "@/configable/Configable";
import { initValues } from "@/configable/utils";
import useStore, { getStoreState } from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useEffect, useState } from "react";
import { hasSameItems } from "@/utils/hasSame";

const ObjectParameter = () => {
  const [structures, setStructures] = useState<RequestStructureParameter[]>([]);
  const [values, setValues] = useState<Record<string, ConfigableValue>>({});

  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === state.mainSelectedItemId),
  );

  useEffect(() => {
    if (!selectedItem) {
      setStructures([]);
      setValues({});
      return;
    }
    window.main
      .requestParameterStruct(
        selectedItem.obj.name,
        selectedItem.obj.parameters,
      )
      .then((struct) => {
        setStructures(struct);
        setValues(initValues(struct, selectedItem.obj.parameters));
      })
      .catch(console.error);
    // selectedItemId が変わったときだけ初期化する
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId]);

  const handleChange = async (newValues: Record<string, ConfigableValue>) => {
    if (!selectedItemId || !selectedItem) return;
    setValues(newValues);

    try {
      const struct = await window.main.requestParameterStruct(
        selectedItem.obj.name,
        newValues,
      );
      if (
        hasSameItems(
          struct.map((p) => p.id),
          structures.map((p) => p.id),
        )
      ) {
        const timeline = (await getStoreState()).timelineLayers;
        setTimelineLayers(
          timeline.map((layer) =>
            layer.id === selectedItemId
              ? { ...layer, obj: { ...layer.obj, parameters: newValues } }
              : layer,
          ),
        );
      } else {
        setStructures(struct);
        // 追加のパラメータを初期化する
        setValues((prevValues) => ({
          ...initValues(struct, newValues),
          ...prevValues,
        }));
      }
    } catch (error) {
      console.error("Error fetching parameter structure:", error);
    }
  };

  return (
    <Configable
      structures={structures}
      value={values}
      onChange={handleChange}
    />
  );
};

export default ObjectParameter;
