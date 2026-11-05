ARG BASE_IMAGE
FROM ${BASE_IMAGE}
RUN apt-get update && apt-get install -y --no-install-recommends qemu-system-arm qemu-utils qemu-efi-aarch64 cloud-image-utils openssh-client curl && rm -rf /var/lib/apt/lists/*
CMD ["sleep", "infinity"]
