<template>
  <section class="docker-helper">
    <h2>Docker Registry</h2>
    <p v-if="isProxy">
      Pull public images through this proxy cache. Authenticate once, then pull as usual.
    </p>
    <p v-else>
      Push images to this hosted registry. Authenticate once, tag your image, then push.
    </p>
    <CodeMenu :snippets="snippets" defaultTab="docker-proxy" />
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import CodeMenu from "@/components/core/code/CodeMenu.vue";
import type { RepositoryWithStorageName } from "@/types/repository";
import { createDockerSetupSnippets } from "./DockerRepositoryHelpers";

const props = defineProps<{ repository: RepositoryWithStorageName }>();

const isProxy = computed(() => props.repository.repository_kind?.toLowerCase() === "proxy");
const snippets = computed(() => createDockerSetupSnippets(props.repository));
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

.docker-helper {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;

  h2 {
    margin: 0;
    color: $primary;
  }
}
</style>
