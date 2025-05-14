
ifneq ($(shell which docker-compose 2>/dev/null),)
    DOCKER_COMPOSE := docker-compose
else
    DOCKER_COMPOSE := docker compose
endif

install:
	$(DOCKER_COMPOSE) up -d

remove:
	@chmod +x confirm_remove.sh
	@./confirm_remove.sh

start:
	$(DOCKER_COMPOSE) start
startAndBuild: 
	$(DOCKER_COMPOSE) up -d --build

stop:
	$(DOCKER_COMPOSE) stop

update:
	# Calls the LLM update script
	chmod +x update_ollama_models.sh
	@./update_ollama_models.sh
	@git pull
	$(DOCKER_COMPOSE) down
	# Make sure the ollama-webui container is stopped before rebuilding
	@docker stop open-webui || true
	$(DOCKER_COMPOSE) up --build -d
	$(DOCKER_COMPOSE) start

VERSION := $(shell git rev-parse --short HEAD)
REMOTE := nvidia@gpu
REMOTE_PATH := ~/work/open-webui
DOCKER_REGISTRY := registry.lazycat.cloud/x/open-webui
DOCKER_NAME := open-webui
ENV_PROXY := http://192.168.1.200:7890

build-multiarch:
	docker buildx build \
	--platform linux/amd64,linux/arm64 \
	-t registry.lazycat.cloud/x/open-webui:$(VERSION) \
	-t registry.lazycat.cloud/x/open-webui:latest \
	--push .

sync-from-arm:
	rsync -arvzlt --delete --exclude-from=.rsyncignore $(REMOTE):$(REMOTE_PATH)/ ./

sync-to-arm:
	ssh -t $(REMOTE) "mkdir -p $(REMOTE_PATH)"
	rsync -arvzlt --delete --exclude-from=.rsyncignore ./ $(REMOTE):$(REMOTE_PATH)

sync-clean:
	ssh -t $(REMOTE) "rm -rf $(REMOTE_PATH)"

build: sync-to-arm
	ssh -t $(REMOTE) "cd $(REMOTE_PATH) && \
		docker build \
	    -f Dockerfile \
	    -t $(DOCKER_REGISTRY):$(VERSION) \
	    -t $(DOCKER_REGISTRY):latest \
        --network host \
        --build-arg "HTTP_PROXY=$(ENV_PROXY)" \
        --build-arg "HTTPS_PROXY=$(ENV_PROXY)" \
		--build-arg "ALL_PROXY=$(ENV_PROXY)" \
        --build-arg "http_proxy=$(ENV_PROXY)" \
        --build-arg "https_proxy=$(ENV_PROXY)" \
		--build-arg "all_proxy=$(ENV_PROXY)" \
        --build-arg "NO_PROXY=localhost,192.168.1.200,registry.lazycat.cloud" \
		."

test: compile build
	ssh -t $(REMOTE) "cd $(REMOTE_PATH) && \
		docker run -it --rm \
		--name $(DOCKER_NAME) \
		--network host \
		$(DOCKER_REGISTRY):latest"

inspect:
	ssh -t $(REMOTE) "cd $(REMOTE_PATH) && \
		docker run -it --rm \
		--name $(DOCKER_NAME) \
		--network host \
		$(DOCKER_REGISTRY):latest \
		bash"

push:
	ssh -t $(REMOTE) "cd $(REMOTE_PATH) && \
		docker push $(DOCKER_REGISTRY):$(VERSION) && \
		docker push $(DOCKER_REGISTRY):latest"
