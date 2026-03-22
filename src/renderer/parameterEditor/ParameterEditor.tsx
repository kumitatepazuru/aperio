import type { RequestStructureParameter } from "native";
import { useEffect, useState } from "react";

const ParameterEditor = () => {
  const [structures, setStructures] = useState<Record<string, RequestStructureParameter[]>>({});

  // debug
  useEffect(() => {
    window.main.getParameterStruct("base.test_object").then((struct) => {
      console.log("struct", struct);
      setStructures((prev) => ({ ...prev, "base.test_object": struct }));
    });
  }, []);

  return (
    <div>ParameterEditor</div>
  )
}

export default ParameterEditor;