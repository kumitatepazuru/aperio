import './plugin/shared'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { preloadPluginComponents } from './plugin/loader'

// プラグインコンポーネントは同期的に描画するため、最初のレンダリングより前に読み込んでおく
// TODO: 非同期読み込みとloading画面の作成
await preloadPluginComponents()

createRoot(document.getElementById('root')!).render(
    <App />,
)
