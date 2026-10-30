import useStore from "@aperio/ui/store";
import type {
  PluginComponentArgsMap,
  PluginComponentType,
} from "@aperio/plugin-sdk/types";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { useShallow } from "zustand/shallow";
import { getPluginComponent } from "./loader";

interface PluginComponentProps<T extends PluginComponentType> {
  type: T;
  args: PluginComponentArgsMap[T];
  /** プラグインが登録されていないとき(または拡張・上書き対象)のデフォルト表示 */
  children: ReactNode;
}

/** プラグインの描画例外で画面全体が落ちないよう、失敗したらデフォルト表示に戻す。 */
class PluginErrorBoundary extends Component<
  { pluginId: string; fallback: ReactNode; children: ReactNode },
  { failed: boolean }
> {
  // TODO: エラーが出たときにユーザーが再読み込みできるようにポップアップを表示させる
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(
      `[plugin] ${this.props.pluginId} failed to render`,
      error,
      info.componentStack,
    );
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

/**
 * `type` に一致して登録されたプラグインコンポーネントで children を包んで描画する。
 * 複数あるときは登録順に包み、後のプラグインほど外側になる。
 */
const PluginComponent = <T extends PluginComponentType>({
  type,
  args,
  children,
}: PluginComponentProps<T>) => {
  const pluginIds = useStore(
    useShallow((s) =>
      s.pluginComponents
        .filter((c) => c.componentType === type)
        .map((c) => c.pluginId),
    ),
  );

  return pluginIds.reduce<ReactNode>((inner, pluginId) => {
    const Plugin = getPluginComponent(pluginId, type);
    if (!Plugin) return inner;
    return (
      <PluginErrorBoundary pluginId={pluginId} fallback={inner}>
        <Plugin args={args}>{inner}</Plugin>
      </PluginErrorBoundary>
    );
  }, children);
};

export default PluginComponent;
