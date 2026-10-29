import { DockPanel, Widget, DockLayout } from "@lumino/widgets";
import {
  Children,
  isValidElement,
  useEffect,
  useMemo,
  useRef,
  type FC,
  type ReactElement,
} from "react";
import "@lumino/default-theme/style/index.css";
import { dockWidgetContext } from "./dockWidgetContext";
import { createRoot } from "react-dom/client";

type SeriarizableITabAreaConfig = Omit<DockLayout.ITabAreaConfig, "widgets"> & {
  widgets: string[]; // widgetのIDの配列に置き換える
};
type SerializableISplitAreaConfig = Omit<
  DockLayout.ISplitAreaConfig,
  "children"
> & {
  children: SerializableAreaConfig[];
};
type SerializableAreaConfig =
  | SeriarizableITabAreaConfig
  | SerializableISplitAreaConfig;
type SerializableILayoutConfig = {
  main: SerializableAreaConfig | null;
};

const makeWidget = (id: string, title: string, content: ReactElement) => {
  const w = new Widget();
  w.id = id;
  w.title.label = title;
  w.title.closable = true;
  w.addClass("widget");

  const container = document.createElement("div");
  container.className = "w-full h-full";

  // この中身は普通の DOM なので、自由に書いてOK
  w.node.appendChild(container);

  const root = createRoot(container);
  root.render(
    <dockWidgetContext.Provider value={w}>
      {content}
    </dockWidgetContext.Provider>,
  );

  return { widget: w, unmount: () => root.unmount() };
};

const Dock: FC<{ children: ReactElement | ReactElement[] }> = ({
  children,
}) => {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const components = useMemo(
    () => Children.toArray(children).filter(isValidElement),
    [children],
  );
  const isInitialized = useRef(false);

  useEffect(() => {
    if (!hostRef.current) return;

    const dock = new DockPanel();
    dock.addClass("grow-1");
    dock.addClass("shrink-1");
    dock.layoutModified.connect(() => {
      if (!isInitialized.current) {
        // 初回のlayoutModifiedは無視する（初期レイアウトの保存を防止）
        return;
      }

      // 内部のWidgets型がjsonに変換できないため置き換える
      const replacer = (
        areaConfig: DockLayout.AreaConfig,
      ): SerializableAreaConfig => {
        if (areaConfig.type === "tab-area") {
          // TabArea
          return {
            ...areaConfig,
            widgets: areaConfig.widgets.map((widget) => widget.id),
          };
        } else {
          // SplitArea
          return {
            ...areaConfig,
            children: areaConfig.children.map(replacer),
          };
        }
      };

      const config = dock.saveLayout();
      let serializableConfig: SerializableILayoutConfig = { main: null };
      if (config.main) {
        serializableConfig = {
          main: replacer(config.main),
        };
      }
      console.log("Layout modified, saving config:", serializableConfig);
      window.main.saveConfig({ dockLayout: serializableConfig });
    });

    const widgets = components
      .map((child, index) => {
        const title = `Untitled ${index + 1}`;
        if (!child.key) {
          console.warn("Child component is missing a key prop");
          return null;
        }

        const widget = makeWidget(child.key, title, child);
        if (index === 0) {
          dock.addWidget(widget.widget);
        } else {
          dock.addWidget(widget.widget, {
            mode: "split-right",
          });
        }

        return widget;
      })
      .filter((w) => w !== null);

    Widget.attach(dock, hostRef.current);

    const onResize = () => dock.update();
    window.addEventListener("resize", onResize);

    // 初期レイアウトの読み込み
    window.main.getConfig().then((config) => {
      if (!config.dockLayout) return;
      const deserializer = (
        areaConfig: SerializableAreaConfig,
      ): DockLayout.AreaConfig => {
        if (areaConfig.type === "tab-area") {
          // TabArea
          const areaWidgets = areaConfig.widgets
            .map((id) => {
              const found = widgets.find((w) => w.widget.id === id);
              return found ? found.widget : null;
            })
            .filter((w): w is Widget => !!w);

          return {
            ...areaConfig,
            widgets: areaWidgets,
          };
        } else {
          // SplitArea
          return {
            ...areaConfig,
            children: areaConfig.children.map(deserializer),
          };
        }
      };

      let deserializedConfig: DockLayout.ILayoutConfig = { main: null };
      if (config.dockLayout.main) {
        deserializedConfig = {
          main: deserializer(config.dockLayout.main),
        };
      }
      dock.restoreLayout(deserializedConfig);
    });

    isInitialized.current = true;
    dock.update();

    return () => {
      widgets.forEach(({ widget, unmount }) => {
        // Reactのバグ？でsynchronouslyでunmountすると警告が出るため非同期で対処
        // https://github.com/facebook/react/issues/25675#issuecomment-1363957941
        setTimeout(() => unmount(), 0);
        widget.dispose();
      });
      dock.dispose();
      window.removeEventListener("resize", onResize);
    };
  }, [components]);

  return (
    <div ref={hostRef} className="flex overflow-hidden w-screen h-screen" />
  );
};

export default Dock;
