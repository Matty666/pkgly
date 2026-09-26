<template>
  <main>
    <nav aria-label="Breadcrumb" class="storage-crumbs">
      <RouterLink to="/admin/storages">Storages</RouterLink>
      <span aria-hidden="true">/</span>
      <span aria-current="page">{{ storage?.name ?? "Storage" }}</span>
    </nav>
    <div
      v-if="storage"
      id="storage">
      <TwoByFormBox>
        <ReadOnlyField label="Name" :value="storage.name" />
        <ReadOnlyField label="Storage Type" :value="storage.storage_type" />
      </TwoByFormBox>
      <component
        :is="storageComponent"
        v-model="storage.config.settings"></component>
    </div>

    <section
      v-if="storage"
      class="storage-danger-zone">
      <v-btn
        color="error"
        variant="flat"
        class="text-none"
        :loading="isDeleting"
        data-testid="storage-delete"
        @click="requestDelete">
        <v-icon
          class="mr-2"
          icon="mdi-delete-outline" />
        Delete Storage
      </v-btn>
    </section>

    <v-dialog
      v-model="isConfirmDialogOpen"
      max-width="500"
      data-testid="storage-delete-dialog">
      <v-card>
        <v-card-title class="text-h6">
          Delete storage "{{ storage?.name ?? "this storage" }}"?
        </v-card-title>
        <v-card-text>
          <p class="mb-2">
            This storage contains {{ repositoryCount }}
            {{ repositoryCount === 1 ? "repository" : "repositories" }}.
            All contained repositories and packages will be permanently deleted from disk.
          </p>
          <p class="mb-0 font-weight-medium">This action cannot be undone.</p>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn
            variant="text"
            class="text-none"
            :disabled="isDeleting"
            data-testid="storage-delete-cancel"
            @click="closeConfirmDialog">
            Cancel
          </v-btn>
          <v-btn
            color="error"
            variant="flat"
            class="text-none"
            :loading="isDeleting"
            data-testid="storage-delete-confirm"
            @click="confirmDelete">
            Delete storage and contents
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </main>
</template>
<script setup lang="ts">
import ReadOnlyField from "@/components/form/ReadOnlyField.vue";
import TwoByFormBox from "@/components/form/TwoByFormBox.vue";
import { storageTypes, type StorageItem } from "@/components/nr/storage/storageTypes";
import http from "@/http";
import router from "@/router";
import { useAlertsStore } from "@/stores/alerts";
import { useRepositoryStore } from "@/stores/repositories";
import { computed, ref } from "vue";

interface StorageDeleteError {
  response?: {
    status?: number;
    data?: {
      message?: string;
      details?: {
        code?: string;
        repository_count?: number;
      };
    };
  };
}

const storageId = router.currentRoute.value.params.id as string;

const alerts = useAlertsStore();
const repositoryStore = useRepositoryStore();
const storage = ref<StorageItem | undefined>(undefined);
const isDeleting = ref(false);
const isConfirmDialogOpen = ref(false);
const repositoryCount = ref(0);

const storageComponent = computed(() => {
  if (!storage.value) {
    return undefined;
  }
  return storageTypes.find((type) => type.value === storage.value?.storage_type)?.updateComponent;
});

async function getStorage() {
  await http.get(`/api/storage/${storageId}`).then((response) => {
    storage.value = response.data;
  });
}

async function requestDelete() {
  await performDelete(false);
}

async function confirmDelete() {
  await performDelete(true);
}

function closeConfirmDialog() {
  isConfirmDialogOpen.value = false;
}

async function performDelete(cascade: boolean) {
  if (isDeleting.value) {
    return;
  }
  isDeleting.value = true;
  try {
    await http.delete(`/api/storage/${storageId}`, { params: { cascade } });
    repositoryStore.removeStorage(storageId);
    alerts.success("Storage deleted", "The storage has been deleted.");
    closeConfirmDialog();
    router.push({ name: "StorageList" });
  } catch (error) {
    const response = (error as StorageDeleteError).response;
    const details = response?.data?.details;
    if (
      !cascade
      && response?.status === 409
      && details?.code === "storage_not_empty"
    ) {
      repositoryCount.value = details.repository_count ?? 0;
      isConfirmDialogOpen.value = true;
    } else if (
      details?.code === "storage_cleanup_failed"
      || details?.code === "storage_backend_unavailable"
    ) {
      alerts.error(
        "Storage deletion failed",
        response?.data?.message ?? "An error occurred while deleting the storage.",
      );
    } else {
      console.error(error);
      alerts.error("Failed to delete storage", "An error occurred while deleting the storage.");
    }
  } finally {
    isDeleting.value = false;
  }
}

getStorage();
</script>
<style scoped lang="scss">
#storage {
  padding: 1rem;
}

.storage-crumbs {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 1rem 1rem 0;
  font-size: 0.875rem;
  color: var(--nr-text-secondary);
}

.storage-crumbs [aria-current="page"] {
  color: var(--nr-text-primary);
  font-weight: 500;
}

.storage-danger-zone {
  padding: 0 1rem 1rem;
}
</style>
