<template>
  <div>
    <ReadOnlyField label="Path" :value="model.path" />
  </div>
</template>

<script setup lang="ts">
import ReadOnlyField from "@/components/form/ReadOnlyField.vue";

const model = defineModel<any>();

</script>
