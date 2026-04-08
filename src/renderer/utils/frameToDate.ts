const frameToDate = (frame: number, fps: number): string => {
  const totalSeconds = frame / fps;
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = Math.floor(totalSeconds % 60);
  const milliseconds = Math.floor(
    (totalSeconds - Math.floor(totalSeconds)) * 1000,
  );

  const pad = (num: number, size: number) => num.toString().padStart(size, "0");

  return `${pad(minutes, 2)}:${pad(seconds, 2)}.${pad(milliseconds, 3)}`;
};

export default frameToDate;
