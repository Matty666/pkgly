<!-- ABOUTME: Renders a repository ecosystem icon without external navigation. -->
<!-- ABOUTME: Keeps brand marks decorative so setup links stay local. -->
<template>
  <span
    :title="icon.name"
    role="img"
    :aria-label="`${icon.name} repository`">
    <component
      :is="icon.component"
      v-bind="icon.props"
  /></span>
</template>
<script setup lang="ts">
import type { RepositoryIconDef } from "@/types/repository";
import type { PropType } from "vue";
defineProps({
  icon: {
    type: Object as PropType<RepositoryIconDef>,
    required: true,
  },
});
</script>

<style lang="scss" scoped>
span {
  display: inline-block;
  margin: 0.5rem;
}
</style>
