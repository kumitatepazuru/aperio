const Loading = () => {
  return (
    <div className="flex items-center justify-center w-screen h-screen">
      <div className="flex gap-3 items-center">
        <span className="loading loading-spinner loading-md"></span>
        <span>データ同期中...</span>
      </div>
    </div>
  );
};

export default Loading;