<!-- ABOUTME: Displays a styled 404 page when no route matches the requested URL. -->
<!-- ABOUTME: Provides a clear fallback with navigation back to the home page. -->
<template>
  <v-container class="not-found-page" fluid>
    <v-row justify="center" align="center" class="fill-height">
      <v-col cols="12" md="6" class="text-center">
        <div class="not-found-page__code">404</div>
        <h1 class="text-h3 font-weight-bold mb-2">Page Not Found</h1>
        <p class="text-body-1 text-medium-emphasis mb-6">
          The page you are looking for does not exist or has been moved.
        </p>
        <v-btn
          :to="{ name: 'home' }"
          color="primary"
          variant="flat"
          size="large"
          class="text-none"
          prepend-icon="mdi-arrow-left">
          Back to Home
        </v-btn>
      </v-col>
    </v-row>
  </v-container>
</template>
<style scoped lang="scss">
.not-found-page {
  min-height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

.not-found-page__code {
  font-size: 6rem;
  font-weight: 700;
  line-height: 1;
  color: var(--nr-primary);
  opacity: 0.15;
  margin-bottom: -1.5rem;
  user-select: none;
}
</style>
