<template>
  <div class="local-config">
    <TextInput
      id="id"
      v-model="model.path"
      required
      autocomplete="none"
      spellcheck="false"
      placeholder="/data/storages/primary">
      Path
    </TextInput>
    <p class="helper">
      Directory inside the Pkgly container where repository data is stored. The path must be
      writable by the server process.
    </p>
  </div>
</template>
<script setup lang="ts">
import TextInput from "@/components/form/text/TextInput.vue";
import http from "@/http";
const model = defineModel<any>();

async function getDefaultPath() {
  await http.post("/api/storage/local/path-helper", {}).then((response) => {
    model.value.path = response.data.value;
  });
}
getDefaultPath();
</script>

<style scoped lang="scss">
.local-config {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.helper {
  margin: 0;
  font-size: 0.875rem;
  color: var(--nr-text-secondary);
}
</style>
