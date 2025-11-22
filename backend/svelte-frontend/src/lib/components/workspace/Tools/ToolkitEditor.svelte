<script>
	import { getContext, onMount, tick } from 'svelte';

	const i18n = getContext('i18n');

	import { goto } from '$app/navigation';
	import { user } from '$lib/stores';

	import CodeEditor from '$lib/components/common/CodeEditor.svelte';
	import ConfirmDialog from '$lib/components/common/ConfirmDialog.svelte';
	import ChevronLeft from '$lib/components/icons/ChevronLeft.svelte';
	import Tooltip from '$lib/components/common/Tooltip.svelte';
	import LockClosed from '$lib/components/icons/LockClosed.svelte';
	import AccessControlModal from '../common/AccessControlModal.svelte';

	let formElement = null;
	let loading = false;

	let showConfirm = false;
	let showAccessControlModal = false;

	export let edit = false;
	export let clone = false;

	export let onSave = () => {};

	export let id = '';
	export let name = '';
	export let meta = {
		description: ''
	};
	export let content = '';
	export let accessControl = {};

	let _content = '';

	$: if (content) {
		updateContent();
	}

	const updateContent = () => {
		_content = content;
	};

	$: if (name && !edit && !clone) {
		id = name.replace(/\s+/g, '_').toLowerCase();
	}

	let codeEditor;
	let boilerplate = `{
  "name": "My Custom Tools",
  "description": "A collection of custom tools for various tasks",
  "version": "1.0.0",
  "tools": [
    {
      "name": "get_current_time",
      "description": "Get the current date and time in a human-readable format",
      "type": "function",
      "parameters": {},
      "handler": {
        "type": "built_in",
        "function": "datetime.now"
      }
    },
    {
      "name": "get_weather",
      "description": "Get the current weather for a given city",
      "type": "http_api",
      "parameters": {
        "city": {
          "type": "string",
          "description": "The city name (e.g., 'New York, NY')",
          "required": true
        }
      },
      "handler": {
        "type": "http",
        "method": "GET",
        "url": "https://api.openweathermap.org/data/2.5/weather",
        "params": {
          "q": "{{city}}",
          "appid": "{{env.OPENWEATHER_API_KEY}}",
          "units": "metric"
        },
        "response": {
          "transform": "Weather in {{params.city}}: {{body.main.temp}}°C, {{body.weather[0].description}}"
        }
      }
    },
    {
      "name": "calculator",
      "description": "Calculate the result of a mathematical equation",
      "type": "expression",
      "parameters": {
        "equation": {
          "type": "string",
          "description": "The mathematical equation to calculate (e.g., '2 + 2 * 3')",
          "required": true
        }
      },
      "handler": {
        "type": "expression",
        "engine": "eval",
        "expression": "{{equation}}"
      }
    },
    {
      "name": "get_user_info",
      "description": "Get the current user's name, email, and ID",
      "type": "context",
      "parameters": {},
      "handler": {
        "type": "context",
        "template": "User: {{user.name}} (ID: {{user.id}}) (Email: {{user.email}})"
      }
    },
    {
      "name": "search_web",
      "description": "Search the web using DuckDuckGo",
      "type": "http_api",
      "parameters": {
        "query": {
          "type": "string",
          "description": "The search query",
          "required": true
        }
      },
      "handler": {
        "type": "http",
        "method": "GET",
        "url": "https://api.duckduckgo.com/",
        "params": {
          "q": "{{query}}",
          "format": "json"
        }
      }
    },
    {
      "name": "mcp_tool_example",
      "description": "Example of calling an MCP (Model Context Protocol) server",
      "type": "mcp",
      "parameters": {
        "input": {
          "type": "string",
          "description": "Input to the MCP tool",
          "required": true
        }
      },
      "handler": {
        "type": "mcp",
        "server": "my_mcp_server",
        "tool": "process_data"
      }
    }
  ],
  "mcp_servers": {
    "my_mcp_server": {
      "url": "http://localhost:3000/mcp",
      "auth_type": "bearer",
      "auth_token": "{{env.MCP_SERVER_TOKEN}}"
    }
  },
  "environment": {
    "required": [
      "OPENWEATHER_API_KEY"
    ],
    "optional": [
      "MCP_SERVER_TOKEN"
    ]
  }
}
`;

	const saveHandler = async () => {
		loading = true;
		onSave({
			id,
			name,
			meta,
			content,
			access_control: accessControl
		});
	};

	const submitHandler = async () => {
		if (codeEditor) {
			content = _content;
			await tick();

			const res = await codeEditor.formatCodeHandler();
			await tick();

			content = _content;
			await tick();

			if (res) {
				console.log('Code formatted successfully');

				saveHandler();
			}
		}
	};
</script>

<AccessControlModal
	bind:show={showAccessControlModal}
	bind:accessControl
	accessRoles={['read', 'write']}
	allowPublic={$user?.permissions?.sharing?.public_tools || $user?.role === 'admin'}
/>

<div class=" flex flex-col justify-between w-full overflow-y-auto h-full">
	<div class="mx-auto w-full md:px-0 h-full">
		<form
			bind:this={formElement}
			class=" flex flex-col max-h-[100dvh] h-full"
			on:submit|preventDefault={() => {
				if (edit) {
					submitHandler();
				} else {
					showConfirm = true;
				}
			}}
		>
			<div class="flex flex-col flex-1 overflow-auto h-0 rounded-lg">
				<div class="w-full mb-2 flex flex-col gap-0.5">
					<div class="flex w-full items-center">
						<div class=" shrink-0 mr-2">
							<Tooltip content={$i18n.t('Back')}>
								<button
									class="w-full text-left text-sm py-1.5 px-1 rounded-lg dark:text-gray-300 dark:hover:text-white hover:bg-black/5 dark:hover:bg-gray-850"
									on:click={() => {
										goto('/workspace/tools');
									}}
									type="button"
								>
									<ChevronLeft strokeWidth="2.5" />
								</button>
							</Tooltip>
						</div>

						<div class="flex-1">
							<Tooltip content={$i18n.t('e.g. My Tools')} placement="top-start">
								<input
									class="w-full text-2xl font-medium bg-transparent outline-hidden font-primary"
									type="text"
									placeholder={$i18n.t('Tool Name')}
									bind:value={name}
									required
								/>
							</Tooltip>
						</div>

						<div class="self-center shrink-0">
							<button
								class="bg-gray-50 hover:bg-gray-100 text-black dark:bg-gray-850 dark:hover:bg-gray-800 dark:text-white transition px-2 py-1 rounded-full flex gap-1 items-center"
								type="button"
								on:click={() => {
									showAccessControlModal = true;
								}}
							>
								<LockClosed strokeWidth="2.5" className="size-3.5" />

								<div class="text-sm font-medium shrink-0">
									{$i18n.t('Access')}
								</div>
							</button>
						</div>
					</div>

					<div class=" flex gap-2 px-1 items-center">
						{#if edit}
							<div class="text-sm text-gray-500 shrink-0">
								{id}
							</div>
						{:else}
							<Tooltip className="w-full" content={$i18n.t('e.g. my_tools')} placement="top-start">
								<input
									class="w-full text-sm disabled:text-gray-500 bg-transparent outline-hidden"
									type="text"
									placeholder={$i18n.t('Tool ID')}
									bind:value={id}
									required
									disabled={edit}
								/>
							</Tooltip>
						{/if}

						<Tooltip
							className="w-full self-center items-center flex"
							content={$i18n.t('e.g. Tools for performing various operations')}
							placement="top-start"
						>
							<input
								class="w-full text-sm bg-transparent outline-hidden"
								type="text"
								placeholder={$i18n.t('Tool Description')}
								bind:value={meta.description}
								required
							/>
						</Tooltip>
					</div>
				</div>

				<div class="mb-2 flex-1 overflow-auto h-0 rounded-lg">
					<CodeEditor
						bind:this={codeEditor}
						value={content}
						lang="json"
						{boilerplate}
						onChange={(e) => {
							_content = e;
						}}
						onSave={async () => {
							if (formElement) {
								formElement.requestSubmit();
							}
						}}
					/>
				</div>

				<div class="pb-3 flex justify-between">
					<div class="flex-1 pr-3">
						<div class="text-xs text-gray-500 line-clamp-2">
							<span class=" font-semibold dark:text-gray-200">{$i18n.t('Warning:')}</span>
							{$i18n.t('Tools are a function calling system with arbitrary code execution')} <br />—
							<span class=" font-medium dark:text-gray-400"
								>{$i18n.t(`don't install random tools from sources you don't trust.`)}</span
							>
						</div>
					</div>

					<button
						class="px-3.5 py-1.5 text-sm font-medium bg-black hover:bg-gray-900 text-white dark:bg-white dark:text-black dark:hover:bg-gray-100 transition rounded-full"
						type="submit"
					>
						{$i18n.t('Save')}
					</button>
				</div>
			</div>
		</form>
	</div>
</div>

<ConfirmDialog
	bind:show={showConfirm}
	on:confirm={() => {
		submitHandler();
	}}
>
	<div class="text-sm text-gray-500">
		<div class=" bg-yellow-500/20 text-yellow-700 dark:text-yellow-200 rounded-lg px-4 py-3">
			<div>{$i18n.t('Please carefully review the following warnings:')}</div>

			<ul class=" mt-1 list-disc pl-4 text-xs">
				<li>
					{$i18n.t('Tools have a function calling system that allows arbitrary code execution.')}
				</li>
				<li>{$i18n.t('Do not install tools from sources you do not fully trust.')}</li>
			</ul>
		</div>

		<div class="my-3">
			{$i18n.t(
				'I acknowledge that I have read and I understand the implications of my action. I am aware of the risks associated with executing arbitrary code and I have verified the trustworthiness of the source.'
			)}
		</div>
	</div>
</ConfirmDialog>
