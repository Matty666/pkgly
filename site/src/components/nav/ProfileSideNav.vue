<template>
  <SideNav>
    <SideNavElement
      to="/profile"
      routeName="profile">
      <font-awesome-icon icon="fa-solid fa-user" />
      <span>Profile</span>
    </SideNavElement>
    <SideNavElement
      to="/profile/login"
      routeName="profileLogin">
      <font-awesome-icon icon="fa-solid fa-right-to-bracket" />
      <span>Login</span>
    </SideNavElement>
    <SideNavElement
      to="/profile/tokens"
      routeName="profileTokens"
      :activeTag="profileTokenTag">
      <font-awesome-icon icon="fa-solid fa-key" />
      <span>Tokens</span>
    </SideNavElement>
  </SideNav>
</template>
<script setup lang="ts">
import { profileTokenTag } from "@/views/profile/profileRoutes";
import SideNav from "./sideNav/SideNav.vue";
import SideNavElement from "./sideNav/SideNavElement.vue";
</script>

<style scoped lang="scss">
// ProfileSideNav uses the new SideNav component which already handles proper theming
</style>
