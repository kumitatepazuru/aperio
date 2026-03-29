import type { RequestStructureParameter } from "native";
import { useEffect, useRef, useState, type FC } from "react";
import { getDefaultValue, type ConfigableValue } from "./utils";
import { create } from "zustand";
import ConfigableRow from "./ConfigableRow";

const initValues = (
  structures: RequestStructureParameter[],
  initial: Record<string, ConfigableValue>,
): Record<string, ConfigableValue> => {
  const record: Record<string, ConfigableValue> = {};
  for (const param of structures) {
    record[param.id] =
      param.id in initial ? initial[param.id] : getDefaultValue(param);
  }
  return record;
};

export type ConfigStoreState = {
  values: Record<string, ConfigableValue>;
  setValue: (id: string, value: ConfigableValue) => void;
  setAll: (values: Record<string, ConfigableValue>) => void;
};

const Configable: FC<{
  structures: RequestStructureParameter[];
  initialValues: Record<string, ConfigableValue>;
  resetKey?: string;
  onChange?: (values: Record<string, ConfigableValue>) => void;
  onInit?: (values: Record<string, ConfigableValue>) => void;
}> = ({ structures, initialValues, resetKey, onChange, onInit }) => {
  const initialValuesRef = useRef(initialValues);
  initialValuesRef.current = initialValues;

  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const onInitRef = useRef(onInit);
  onInitRef.current = onInit;

  const [useConfigStore] = useState(() =>
    create<ConfigStoreState>((set) => ({
      values: initValues(structures, initialValues),
      setValue: (id, value) =>
        set((state) => ({ values: { ...state.values, [id]: value } })),
      setAll: (newValues) => set({ values: newValues }),
    })),
  );

  // valuesが変わったら外部のonChangeに通知
  useEffect(() => {
    return useConfigStore.subscribe((state, prevState) => {
      if (state.values !== prevState.values) {
        onChangeRef.current?.(state.values);
      }
    });
  }, [useConfigStore]);

  // structuresまたはアイテムが変わったときにvaluesを初期化する
  useEffect(() => {
    console.log(
      "structures or resetKey changed, reinitializing values",
      structures,
    );
    const values = initValues(structures, initialValuesRef.current);
    useConfigStore.getState().setAll(values);
    onInitRef.current?.(values);
  }, [useConfigStore, structures, resetKey]);

  // structuresのすべてのidがストアに存在するときだけConfigableRowを描画する
  const allValuesReady = useConfigStore((state) =>
    structures.every((p) => p.id in state.values),
  );

  return (
    <div className="grid grid-cols-[5em_1fr_auto] gap-2 items-center text-sm">
      {allValuesReady &&
        structures.map((param) => (
          <ConfigableRow
            key={param.id}
            param={param}
            useConfigStore={useConfigStore}
          />
        ))}
    </div>
  );
};

export default Configable;
