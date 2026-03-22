import { Suspense, type JSX } from "react";
import ChangeResolution from "./dialogs/ChangeResolution";
import Loading from "@shared/Loading";

const App = () => {
  const currentURL = window.location.href;
  const dialogId = new URL(currentURL).searchParams.get("id");
  let returnValue: JSX.Element = (
    <div>specified dialog is not found. id: {dialogId}</div>
  );

  switch (dialogId) {
    case "change-resolution":
      returnValue = <ChangeResolution />;
  }

  return <Suspense fallback={<Loading />}>{returnValue}</Suspense>;
};

export default App;
