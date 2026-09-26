<template>
  <section class="s3-config">
    <TwoByFormBox>
      <ReadOnlyField label="Bucket Name" :value="model.bucket_name" />
      <ReadOnlyField label="Region / Endpoint Mode" :value="regionDisplay" />
    </TwoByFormBox>

    <TwoByFormBox v-if="model.endpoint">
      <ReadOnlyField label="Endpoint URL" :value="endpointDisplay" />
      <ReadOnlyField label="Custom Region Name" :value="customRegionDisplay" />
    </TwoByFormBox>

    <TwoByFormBox>
      <ReadOnlyField label="Access Key" :value="model.credentials.access_key" />
      <ReadOnlyField label="Secret Key" :value="maskedSecret" />
    </TwoByFormBox>

    <ReadOnlyField label="Session Token" :value="maskedSession" />

    <TwoByFormBox>
      <ReadOnlyField label="Role ARN" :value="model.credentials.role_arn" />
      <ReadOnlyField label="Role Session Name" :value="model.credentials.role_session_name" />
    </TwoByFormBox>

    <ReadOnlyField label="External ID" :value="model.credentials.external_id" />

    <ReadOnlyField label="Addressing Mode" :value="model.path_style ? 'Path-style' : 'Virtual-hosted'" />

    <ReadOnlyField label="Disk Cache" :value="model.cache.enabled ? 'Enabled' : 'Disabled'" />

    <TwoByFormBox v-if="model.cache.enabled">
      <ReadOnlyField label="Cache Directory" :value="model.cache.path" />
      <ReadOnlyField label="Max Size" :value="formattedMaxSize" />
    </TwoByFormBox>
    <ReadOnlyField
      v-if="model.cache.enabled"
      label="Max Cached Entries"
      :value="String(model.cache.max_entries)" />
  </section>
</template>

<script setup lang="ts">
import ReadOnlyField from "@/components/form/ReadOnlyField.vue";
import TwoByFormBox from "@/components/form/TwoByFormBox.vue";
import { computed } from "vue";
import type { S3StorageSettings } from "@/components/nr/storage/storageTypes";

const model = defineModel<S3StorageSettings>({
  required: true,
});

if (!model.value.credentials) {
  model.value.credentials = {};
}
model.value.credentials.access_key ??= "";
model.value.credentials.secret_key ??= "";
model.value.credentials.session_token ??= "";
model.value.credentials.role_arn ??= "";
model.value.credentials.role_session_name ??= "";
model.value.credentials.external_id ??= "";
if (typeof model.value.path_style !== "boolean") {
  model.value.path_style = true;
}
model.value.cache ??= {
  enabled: false,
  path: "",
  max_bytes: 536870912,
  max_entries: 2048,
};
model.value.cache.path ??= "";
if (typeof model.value.cache.max_bytes !== "number") {
  model.value.cache.max_bytes = 536870912;
}
if (typeof model.value.cache.max_entries !== "number") {
  model.value.cache.max_entries = 2048;
}

const regionDisplay = computed({
  get: () => {
    if (model.value.endpoint) {
      return "Custom endpoint";
    }
    return model.value.region ?? "Region not set";
  },
  set: () => {},
});

const endpointDisplay = computed({
  get: () => model.value.endpoint ?? "",
  set: () => {},
});

const customRegionDisplay = computed({
  get: () => model.value.custom_region ?? "",
  set: () => {},
});

const formattedMaxSize = computed(() => {
  const bytes = model.value.cache.max_bytes;
  if (bytes >= 1024 * 1024 * 1024) {
    return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
  }
  return (bytes / (1024 * 1024)).toFixed(2) + " MB";
});

const maskedSecret = computed(() => {
  const secret = model.value.credentials?.secret_key ?? "";
  if (!secret) {
    return "";
  }
  return "*".repeat(Math.min(secret.length, 12));
});

const maskedSession = computed(() => {
  const token = model.value.credentials?.session_token ?? "";
  if (!token) {
    return "";
  }
  return "*".repeat(Math.min(token.length, 12));
});
</script>

<style scoped lang="scss">
.s3-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
</style>
