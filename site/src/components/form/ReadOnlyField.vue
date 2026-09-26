<!-- ABOUTME: Read-only labeled value with copy button for identifiers and URLs. -->
<!-- ABOUTME: Replaces disabled text inputs used as display surfaces. -->
<template>
  <div class="readonly-field">
    <span class="readonly-field__label">{{ label }}</span>
    <div class="readonly-field__control">
      <code class="readonly-field__value" :title="value ?? undefined">{{ displayValue }}</code>
      <button
        v-if="value"
        type="button"
        class="readonly-field__copy"
        :aria-label="`Copy ${label}`"
        :title="`Copy ${label}`"
        @click="copy">
        <v-icon size="small" aria-hidden="true">mdi-content-copy</v-icon>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useAlertsStore } from "@/stores/alerts";

const props = defineProps<{
  label: string;
  value: string | null | undefined;
}>();

const alerts = useAlertsStore();
const displayValue = computed(() => props.value || "—");

function copy() {
  if (!props.value) {
    return;
  }
  navigator.clipboard.writeText(props.value);
  alerts.success("Copied");
}
</script>

<style scoped lang="scss">
.readonly-field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  min-width: 0;
}

.readonly-field__label {
  font-size: 0.8125rem;
  letter-spacing: 0.02em;
  font-weight: 600;
  color: var(--nr-text-secondary);
  text-transform: uppercase;
}

.readonly-field__control {
  display: flex;
  align-items: stretch;
  max-width: 100%;
}

.readonly-field__value {
  min-width: 0;
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--nr-border-color);
  border-right: 0;
  border-radius: var(--nr-radius-md) 0 0 var(--nr-radius-md);
  background: var(--nr-surface-variant);
  color: var(--nr-text-primary);
  font-family: var(--nr-font-family-mono);
  font-size: 0.875rem;
}

.readonly-field__copy {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--nr-border-color);
  border-radius: 0 var(--nr-radius-md) var(--nr-radius-md) 0;
  background: var(--nr-background);
  color: var(--nr-primary);
  cursor: pointer;
  padding: 0 0.625rem;

  &:hover,
  &:focus-visible {
    background: var(--nr-primary-07);
    outline: none;
    box-shadow: var(--nr-focus-ring);
  }
}
</style>
