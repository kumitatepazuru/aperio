import { createContext, useContext } from "react";
import type { TimelineContextValue } from "@aperio/plugin-sdk/timeline";

export const TimelineContext = createContext<TimelineContextValue | null>(null);

export const useTimeline = (): TimelineContextValue => {
  const value = useContext(TimelineContext);
  if (!value) {
    throw new Error("Can't use useTimeline outside of TimelineContext.Provider");
  }
  return value;
};
