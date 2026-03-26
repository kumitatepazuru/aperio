import { memo, type ReactNode } from "react";
import { LuUndo2 } from "react-icons/lu";
import type { ColorValue, Vec2Value, Vec3Value, Vec4Value } from "./types";
import Float from "./components/Float";
import Int from "./components/Int";
import Bool from "./components/Bool";
import Vec2 from "./components/Vec2";
import Vec3 from "./components/Vec3";
import Vec4 from "./components/Vec4";
import StringInput from "./components/StringInput";
import Color from "./components/Color";
import ListSelect from "./components/ListSelect";
import type { UseBoundStore, StoreApi } from "zustand";
import { getDefaultValue, type ConfigStoreState } from "./useConfigable";
import type { RequestStructureParameter } from "native";

type UseConfigStore = UseBoundStore<StoreApi<ConfigStoreState>>;

const ConfigableRow = memo(
  ({
    param,
    useConfigStore,
  }: {
    param: RequestStructureParameter;
    useConfigStore: UseConfigStore;
  }) => {
    const id = param.id;
    const value = useConfigStore((state: ConfigStoreState) => state.values[id]);
    const setValue = useConfigStore(
      (state: ConfigStoreState) => state.setValue,
    );

    let input: ReactNode;
    switch (param.type) {
      case "Float":
        input = (
          <Float
            value={value as number}
            suffix={param.suffix}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Int":
        input = (
          <Int
            value={value as number}
            suffix={param.suffix}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Bool":
        input = (
          <Bool value={value as boolean} onChange={(v) => setValue(id, v)} />
        );
        break;
      case "Vec2Int":
        input = (
          <Vec2
            value={value as Vec2Value}
            suffix={param.suffix}
            isInt={true}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Vec2Float":
        input = (
          <Vec2
            value={value as Vec2Value}
            suffix={param.suffix}
            isInt={false}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Vec3Int":
        input = (
          <Vec3
            value={value as Vec3Value}
            suffix={param.suffix}
            isInt={true}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Vec3Float":
        input = (
          <Vec3
            value={value as Vec3Value}
            suffix={param.suffix}
            isInt={false}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Vec4Int":
        input = (
          <Vec4
            value={value as Vec4Value}
            suffix={param.suffix}
            isInt={true}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Vec4Float":
        input = (
          <Vec4
            value={value as Vec4Value}
            suffix={param.suffix}
            isInt={false}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "String":
        input = (
          <StringInput
            value={value as string}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "Color":
        input = (
          <Color
            value={value as ColorValue}
            useAlpha={param.useAlpha}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
      case "List":
        input = (
          <ListSelect
            value={value as string}
            values={param.values}
            onChange={(v) => setValue(id, v)}
          />
        );
        break;
    }

    return (
      <>
        <label>{param.title}</label>
        {input}
        <button
          onClick={() => setValue(id, getDefaultValue(param))}
          className="btn btn-square btn-sm h-full min-h-8"
        >
          <LuUndo2 />
        </button>
      </>
    );
  },
);

export default ConfigableRow;
