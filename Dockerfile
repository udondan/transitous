# Runs a static transitous binary on Alpine. By default it is built from the
# source; releases pass `--build-arg BINARY=prebuilt` and copy the musl
# binaries of the release from dist/<arch>/transitous, so no Rust is compiled
# under emulation. `--build-arg PROFILE=dev` builds without optimizations, in
# a fraction of the time.
ARG BINARY=source

FROM rust:1.100-alpine AS source

# ring needs a C compiler and the musl headers.
RUN apk add --no-cache musl-dev

ARG PROFILE=release
WORKDIR /src
COPY . .
# Cargo writes the profile dev to target/debug.
RUN cargo build --profile "$PROFILE" --locked -p transitous \
    && dir=$([ "$PROFILE" = dev ] && echo debug || echo "$PROFILE") \
    && cp "target/$dir/transitous" /usr/local/bin/transitous


FROM scratch AS prebuilt
ARG TARGETARCH
COPY dist/${TARGETARCH}/transitous /usr/local/bin/transitous


# BuildKit only builds the stage that is used.
FROM ${BINARY} AS binary


FROM alpine:3.24

LABEL com.docker.desktop.mcp="true"
LABEL com.docker.desktop.mcp.name="transitous"
# Proves to the MCP Registry that this image belongs to the server of
# crates/transitous/server.json.
LABEL io.modelcontextprotocol.server.name="io.github.udondan/transitous"

# apk upgrade pulls fixes for Alpine packages released after the base image.
# The home directory holds the cache and the usage counter
# (~/.local/state/transitous); mount a volume there to keep the counter
# across containers.
RUN apk upgrade --no-cache && adduser -D -u 10001 transitous
COPY --from=binary /usr/local/bin/transitous /usr/local/bin/transitous
USER 10001
WORKDIR /home/transitous

# The contact for the User-Agent comes from TRANSITOUS_CONTACT. Without it the
# server starts, but every tool answers contact_missing.

# `docker run udondan/transitous <command>` runs the CLI; without a command
# the image starts the MCP server, which is what the Docker MCP Toolkit
# expects.
ENTRYPOINT ["transitous"]
CMD ["mcp"]
