<script setup lang="ts">
import FancyCard from './FancyCard.vue';
import { vFocusTrap } from './directives';
import { formatDate } from './utils';
import { handlers } from './utils';
import { unusedImported } from './utils';
const date = new Date();
</script>

<template>
    <FancyCard />
    <input v-focus-trap />
    <button v-on="handlers">Add</button>
    <div>{{ formatDate(date) }}</div>
</template>
