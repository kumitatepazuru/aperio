FROM python:3.13-slim
WORKDIR /build

RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    --no-install-recommends --no-install-suggests
RUN apt-get install -y libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    ninja-build \
    patchelf \
    --no-install-recommends --no-install-suggests
RUN pip download --no-deps --no-binary opencv-python-headless opencv-python-headless
RUN tar xzvf opencv-python-headless-*.tar.gz
RUN mv opencv-python-headless-*/ opencv

# https://docs.opencv.org/4.x/db/d05/tutorial_config_reference.html
WORKDIR /build/opencv/
ARG CMAKE_ARGS="-D CMAKE_BUILD_TYPE=RELEASE \
    -D CPU_DISPATCH=AVX,AVX2 \
    -D WITH_OPENCL=ON \
    -D WITH_V4L=OFF \
    -D WITH_FFMPEG=OFF \
    -D WITH_GSTREAMER=ON \
    -D HIGHGUI_ENABLE_PLUGINS=OFF \
    -D WITH_GTK=OFF \
    -D WITH_WIN32API=OFF \
    -D BUILD_opencv_python3=ON"

RUN MAKEFLAGS="-j$(nproc)" pip wheel .
