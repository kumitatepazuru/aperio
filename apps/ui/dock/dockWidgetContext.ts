import type { Widget } from "@lumino/widgets";
import { createContext } from "react";

export const dockWidgetContext = createContext<Widget>(null as unknown as Widget);