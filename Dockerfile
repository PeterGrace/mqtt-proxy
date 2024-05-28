FROM docker.io/ubuntu:24.04
ARG TARGETARCH

RUN mkdir -p /opt/app
WORKDIR /opt/app
COPY ./tools/target_arch.sh /opt/app
COPY config.yaml /opt/app/config.yaml
RUN --mount=type=bind,target=/context \
 cp /context/target/$(/opt/app/target_arch.sh)/release/mqtt-proxy /opt/app/mqtt-proxy
CMD ["/opt/app/mqtt-proxy"]
