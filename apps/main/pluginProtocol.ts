import { net, protocol } from "electron";
import * as path from "path";
import { pathToFileURL } from "node:url";
import type { NativeModule } from "./nativeModule";

const SCHEME = "plugin";

/**
 * `plugin://{pluginId}/{type}/{file}` でプラグインのコンポーネントを配信するスキームを登録する。
 */
export function registerPluginScheme() {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: SCHEME,
      privileges: {
        standard: true,
        secure: true,
        supportFetchAPI: true,
        corsEnabled: true,
      },
    },
  ]);
}

function text(status: number, body: string) {
  return new Response(body, {
    status,
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
}

/**
 * `plugin://` の実体を登録する。app の ready 後に呼ぶ。
 * 配信できるのは `addComponent` で登録されたディレクトリ配下のファイルだけ。
 */
export function registerPluginProtocol(nativeModule: NativeModule) {
  protocol.handle(SCHEME, async (request) => {
    const url = new URL(request.url);
    const segments = url.pathname
      .split("/")
      .filter((s) => s.length > 0)
      .map((s) => decodeURIComponent(s));
    const pluginId = decodeURIComponent(url.hostname);
    const [type, ...rest] = segments;
    if (!type) return text(400, "component type is required");

    const component = nativeModule.storeManager
      .getState()
      .pluginComponents.find(
        (c) => c.pluginId === pluginId && c.componentType === type,
      );
    if (!component) return text(404, "component is not registered");

    const root = path.resolve(component.dir);
    const file = path.resolve(root, ...(rest.length > 0 ? rest : ["index.js"]));
    // 登録されたディレクトリの外へ出るパス(`..` など)は配信しない
    if (!file.startsWith(root + path.sep)) return text(403, "forbidden");

    const res = await net.fetch(pathToFileURL(file).toString());
    if (!res.ok) return text(res.status, "not found");

    // dev の renderer (http://localhost) からは cross-origin になるので CORS を許可する
    const headers = new Headers(res.headers);
    headers.set("Access-Control-Allow-Origin", "*");
    if (file.endsWith(".js")) headers.set("Content-Type", "text/javascript");
    return new Response(res.body, { status: res.status, headers });
  });
}
