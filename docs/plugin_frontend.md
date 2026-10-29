# プラグインのフロントエンド(Reactコンポーネント)

Pythonプラグインは、ビルド済みのReactコンポーネントをUIの決まった場所(`type`)に差し込める。
用語は [terms.md](terms.md) の「PluginComponent」を参照。実装例は `plugins/base/frontend/group_item/`(グループオブジェクト)。

## 仕組み

1. Pythonプラグインが `aperio_plugin.plugin_manager.add_component(plugin, dir, type)` で `index.js` のあるディレクトリを登録する。登録情報は store の `pluginComponents` に入る
2. UI は起動時(最初の描画前)に、登録されたコンポーネントを `plugin://{pluginId}/{type}/index.js` から動的 `import()` する
3. ホストの `<PluginComponent type="..." args={...}>デフォルト表示</PluginComponent>` が、`type` が一致するコンポーネントを登録順に children の外側へ重ねて描画する
   - コンポーネントが `children` を描画すれば「拡張」、描画しなければ「上書き」になる
   - 読込中の見た目などの調整はプラグイン側で行う(ホストは Suspense を使わない)
   - 読込に失敗したプラグインや、描画中に例外を出したプラグインはスキップされ、デフォルト表示のままになる
4. `plugin://` が配信するのは、`add_component` で登録されたディレクトリの中のファイルだけ

プラグインの読み込みは起動時だけなので、パッケージを追加・更新したら再起動するまで反映されない。

## Python側

```python
class MyPlugin(MainPluginBase):
    def __init__(self):
        super().__init__()
        self.name = "my_plugin"
        ...
        obj = MyObject()
        aperio_plugin.plugin_manager.add_generator(obj)
        # 登録するプラグイン、index.js があるディレクトリ(相対パスは絶対パスに変換される)、差し込み先の type
        aperio_plugin.plugin_manager.add_component(
            obj, os.path.join(os.path.dirname(__file__), "frontend", "my_item"), "item"
        )
```

- `add_component` は `PluginManager`(`aperio_plugin.plugin_manager`)のメソッドで、メインプラグイン・ジェネレーターどちらのインスタンスも渡せる
- プラグインは wheel で配布されるので、ビルド済みの `dist/` を wheel に含める必要がある。base では `plugins/base/pyproject.toml` の `force-include` で `frontend/group_item/dist` を `aperio_base/frontend/group_item` に取り込んでいる
- `index.js` が無いときは警告を出して登録しない
- `pluginId` は渡したプラグインの `name`。同じ `(pluginId, type)` を再登録すると上書きされる

## フロントエンド側

`plugins/<plugin>/frontend/<name>/` に vite プロジェクトを置く(`group_item` を参考にする)。

```ts
// vite.config.ts
import { aperioPlugin } from "@aperio/plugin-sdk/vite";
import { defineConfig } from "vite";

export default defineConfig({ plugins: [aperioPlugin()] });
```

`aperioPlugin()` が `base: "./"`、`index.js` 1 ファイルの出力(`src/index.tsx` が入口。htmlは使えない)、`@vitejs/plugin-react` を設定する。

```tsx
// src/index.tsx
import type { PluginComponentProps } from "@aperio/plugin-sdk/types";

export default function MyItem({ args, children }: PluginComponentProps<"item">) {
  return (
    <>
      {children}
      {/* args.item, args.left, ... */}
    </>
  );
}
```

- `args` の型は type ごとに決まり、`PluginComponentProps<type>` で参照できる(定義は `packages/plugin-sdk/types.ts`)
- CSS は JS に取り込まれる。Tailwind を使うときは、ホストの見た目を壊さないよう preflight を読み込まない(`group_item/src/index.css` を参照)

### ホストと共有されるモジュール

次の import はビルド時にバンドルされず、ホストが公開しているインスタンスに置き換わる(react が二重に読み込まれると hooks が動かなくなるため)。

`react`, `react/jsx-runtime`, `react-dom`, `zustand`, `zustand/shallow`, `@aperio/plugin-sdk/store`

`@aperio/plugin-sdk/store` はホストの store(`useStore`, `getStoreState`)で、型だけの `packages/plugin-sdk/store.d.ts` を置いてある。IDE では型が効き、実行時はホストのものが使われる。
ホストの `Store` 型(`packages/ui/store.ts`)を変えたら、`store.d.ts` も合わせる。
