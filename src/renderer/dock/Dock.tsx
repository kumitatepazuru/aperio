import { DockPanel, Widget } from "@lumino/widgets";
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

const makeWidget = (title: string, content: ReactElement) => {
  const w = new Widget();
  w.title.label = title;
  w.title.closable = true;
  w.addClass("widget");

  const container = document.createElement("div");

  // この中身は普通の DOM なので、自由に書いてOK
  w.node.appendChild(container);

  const root = createRoot(container);
  root.render(
    <dockWidgetContext.Provider value={w}>{content}</dockWidgetContext.Provider>
  );

  return { widget: w, unmount: () => root.unmount() };
};

const Dock: FC<{ children: ReactElement | ReactElement[] }> = ({
  children,
}) => {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const components = useMemo(
    () => Children.toArray(children).filter(isValidElement),
    [children]
  );

  useEffect(() => {
    if (!hostRef.current) return;

    const dock = new DockPanel();
    dock.addClass("grow-1");
    dock.addClass("shrink-1");

    const widgets = components.map((child, index) => {
      const title = `Untitled ${index + 1}`;
      const content = child;
      const widget = makeWidget(title, content);
      if (index === 0) {
        dock.addWidget(widget.widget);
      } else {
        dock.addWidget(widget.widget, {
          mode: "split-right",
        });
      }

      return widget;
    });

    Widget.attach(dock, hostRef.current);

    const onResize = () => dock.update();
    window.addEventListener("resize", onResize);

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
