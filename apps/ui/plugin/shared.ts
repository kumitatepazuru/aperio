import * as React from "react";
import * as ReactDOM from "react-dom";
import * as ReactJsxRuntime from "react/jsx-runtime";
import * as zustand from "zustand";
import * as zustandShallow from "zustand/shallow";
import useStore, * as store from "@aperio/ui/store";
import * as timeline from "../timeline/TimelineContext";

/**
 * プラグインと共有するモジュール。tree shakingされないようにglobalThis.__APERIO_SHARED__ に置く。
 * キーはプラグイン側の import 指定子 (`@aperio/plugin-sdk/vite` の SHARED_MODULES と一致させる)。
 */
const shared = {
  react: React,
  "react/jsx-runtime": ReactJsxRuntime,
  "react-dom": ReactDOM,
  zustand,
  "zustand/shallow": zustandShallow,
  "@aperio/plugin-sdk/store": { ...store, default: useStore, useStore },
  "@aperio/plugin-sdk/timeline": timeline,
};

(globalThis as unknown as { __APERIO_SHARED__: typeof shared }).__APERIO_SHARED__ =
  shared;
