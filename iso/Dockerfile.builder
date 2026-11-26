FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    xorriso \
    isolinux \
    syslinux-common \
    cpio \
    gzip \
    apt-rdepends \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work
COPY iso/build-iso.sh /usr/local/bin/build-iso.sh
RUN sed -i 's/\r$//' /usr/local/bin/build-iso.sh && chmod +x /usr/local/bin/build-iso.sh

ENTRYPOINT ["/usr/local/bin/build-iso.sh"]
