FROM rust:1.96.0-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /edict
COPY . /edict
WORKDIR /consumer-source
RUN git init && git remote add origin https://github.com/flyingrobots/jedit.git \
    && git fetch --depth=1 origin 19edb6fba94a8fea2dea63aa2f05cffc3e084f97 \
    && git archive FETCH_HEAD edict/replace-range-probes/state-read | tar -x
WORKDIR /echo-source
RUN git init && git remote add origin https://github.com/flyingrobots/echo.git \
    && git fetch --depth=1 origin 49e9efb68001dfd78563d18bac9359a87671e431 \
    && git archive FETCH_HEAD schemas/edict-provider/package/v1 | tar -x
WORKDIR /edict
ENV CARGO_HOME=/build-cache/cargo CARGO_TARGET_DIR=/build-cache/target \
    CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0
CMD ["sh", "scripts/consumer-witnesses/run-jedit-state-read.sh"]
