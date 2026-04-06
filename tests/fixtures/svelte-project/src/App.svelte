<script lang="ts">
import FancyButton from './FancyButton.svelte';
import { isActive } from './utils';
import { tooltip } from './utils';
import * as utils from './utils';
import { unusedImported } from './utils';

let name = 'world';
</script>

<FancyButton />
<button use:tooltip class:active={isActive}>Hover me</button>
<h1>Hello {utils.formatName(name)}!</h1>
