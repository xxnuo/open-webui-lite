<script>
	import { getContext, onMount } from 'svelte';
	const i18n = getContext('i18n');

	import { user } from '$lib/stores';

	import XMark from '$lib/components/icons/XMark.svelte';
	import Modal from '$lib/components/common/Modal.svelte';

	export let show = false;

	let selected = '';

	onMount(async () => {
		if ($user?.role === 'admin') {
			selected = '';
		}
	});
</script>

<Modal size="sm" bind:show>
	<div>
		<div class=" flex justify-between dark:text-gray-100 px-5 pt-4">
			<div class=" text-lg font-medium self-center font-primary">
				{$i18n.t('Manage Models')}
			</div>
			<button
				class="self-center"
				on:click={() => {
					show = false;
				}}
			>
				<XMark className={'size-5'} />
			</button>
		</div>

		<div class="flex flex-col md:flex-row w-full px-3 pb-4 md:space-x-4 dark:text-gray-200">
			<div class=" flex flex-col w-full sm:flex-row sm:justify-center sm:space-x-6">
				<div class=" py-5 text-gray-400 text-xs">
					<div>
						{$i18n.t('No inference engine with management support found')}
					</div>
				</div>
			</div>
		</div>
	</div>
</Modal>
