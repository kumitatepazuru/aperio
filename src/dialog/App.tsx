import ChangeResolution from "./dialogs/ChangeResolution";

const App = () => {
  const currentURL = window.location.href;
  const dialogId = new URL(currentURL).searchParams.get("id");

  switch (dialogId) {
    case "change-resolution":
      return <ChangeResolution />;
    default:
      return <div>specified dialog is not found. id: {dialogId}</div>;
  }
};

export default App;
