<script lang="ts">
	export let show = false;
	export let selectedModelId = '';

	import { marked } from 'marked';
	// Configure marked with extensions
	marked.use({
		breaks: true,
		gfm: true,
		renderer: {
			list(body, ordered, start) {
				const isTaskList = body.includes('data-checked=');

				if (isTaskList) {
					return `<ul data-type="taskList">${body}</ul>`;
				}

				const type = ordered ? 'ol' : 'ul';
				const startatt = ordered && start !== 1 ? ` start="${start}"` : '';
				return `<${type}${startatt}>${body}</${type}>`;
			},

			listitem(text, task, checked) {
				if (task) {
					const checkedAttr = checked ? 'true' : 'false';
					return `<li data-type="taskItem" data-checked="${checkedAttr}">${text}</li>`;
				}
				return `<li>${text}</li>`;
			}
		}
	});

	import { toast } from 'svelte-sonner';

	import { goto } from '$app/navigation';
	import { onMount, tick, getContext } from 'svelte';
	import { v4 as uuidv4 } from 'uuid';

	import {
		OPENAI_API_BASE_URL,
		WEBUI_API_BASE_URL,
		WEBUI_BASE_URL
	} from '$lib/constants';
	import { WEBUI_NAME, config, user, models, settings, socket } from '$lib/stores';

	import { chatCompletion } from '$lib/apis/openai';

	import { splitStream } from '$lib/utils';

	import Messages from '$lib/components/notes/NoteEditor/Chat/Messages.svelte';
	import MessageInput from '$lib/components/channel/MessageInput.svelte';
	import XMark from '$lib/components/icons/XMark.svelte';
	import Tooltip from '$lib/components/common/Tooltip.svelte';
	import Pencil from '$lib/components/icons/Pencil.svelte';
	import PencilSquare from '$lib/components/icons/PencilSquare.svelte';

	const i18n = getContext('i18n');

	export let editor = null;

	export let editing = false;
	export let streaming = false;
	export let stopResponseFlag = false;

	export let note = null;
	export let selectedContent = null;

	export let files = [];
	export let messages = [];

	export let onInsert = (content) => {};
	export let onStop = () => {};
	export let onEdited = () => {};

	export const insertNoteHandler = () => {};
	export let scrollToBottomHandler = () => {};

	let loaded = false;

	let loading = false;

	let messagesContainerElement: HTMLDivElement;

	let system = '';
	let editEnabled = false;
	let chatInputElement = null;

	const DEFAULT_DOCUMENT_EDITOR_PROMPT = `You are an expert document editor.

## Task
Based on the user's instruction, update and enhance the existing notes or selection by incorporating relevant and accurate information from the provided context in the content's primary language. Ensure all edits strictly follow the user’s intent.

## Input Structure
- Existing notes: Enclosed within <notes></notes> XML tags.
- Additional context: Enclosed within <context></context> XML tags.
- Current note selection: Enclosed within <selection></selection> XML tags.
- Editing instruction: Provided in the user message.

## Output Instructions
- If a selection is provided, edit **only** the content within <selection></selection>. Leave unselected parts unchanged.
- If no selection is provided, edit the entire notes.
- Deliver a single, rewritten version of the notes in markdown format.
- Integrate information from the context only if it directly supports the user's instruction.
- Use clear, organized markdown elements: headings, lists, task lists ([ ]) where tasks or checklists are strongly implied, bold and italic text as appropriate.
- Focus on improving clarity, completeness, and usefulness of the notes.
- Return only the final, fully-edited markdown notes—do not include explanations, reasoning, or XML tags.
`;

	let scrolledToBottom = true;
	let activeMessageId = null;
	let activeResponseMessage = null;
	let activeEnhancedContent = null;

	const scrollToBottom = () => {
		if (messagesContainerElement) {
			if (scrolledToBottom) {
				messagesContainerElement.scrollTop = messagesContainerElement.scrollHeight;
			}
		}
	};

	const onScroll = () => {
		if (messagesContainerElement) {
			scrolledToBottom =
				messagesContainerElement.scrollHeight - messagesContainerElement.scrollTop <=
				messagesContainerElement.clientHeight + 10;
		}
	};

	// Socket.IO event handler for real-time streaming (matching main Chat.svelte structure)
	const handleChatEvent = (event, cb) => {
		console.log('Note chat event:', event);

		// Only process events for our active message
		if (event.message_id !== activeMessageId) {
			return;
		}

		const type = event?.data?.type ?? null;
		const data = event?.data?.data ?? null;

		if (type === 'status') {
			// Status updates (connecting, processing, etc.)
			console.log('Status:', data);
		} else if (type === 'chat:message:delta' || type === 'message') {
			// Message chunk received (delta streaming)
			if (activeResponseMessage && data?.content) {
				const chunk = data.content;

				if (editEnabled) {
					// Make sure activeEnhancedContent is initialized
					if (!activeEnhancedContent) {
						activeEnhancedContent = {
							json: null,
							html: '',
							md: ''
						};
					}

					editing = true;
					streaming = true;

					activeEnhancedContent.md = (activeEnhancedContent.md || '') + chunk;
					activeEnhancedContent.html = marked.parse(activeEnhancedContent.md || '');

					if (!selectedContent || !selectedContent?.text) {
						note.data.content.md = activeEnhancedContent.md;
						note.data.content.html = activeEnhancedContent.html;
						note.data.content.json = null;
					}

					scrollToBottomHandler();

					activeResponseMessage.content = `<status title="${$i18n.t('Editing')}" done="false" />`;
					messages = messages;
				} else {
					activeResponseMessage.content += chunk;
					messages = messages;
				}

				tick().then(() => scrollToBottom());
			}
		} else if (type === 'chat:message' || type === 'replace') {
			// Full content replacement
			if (activeResponseMessage && data?.content) {
				activeResponseMessage.content = data.content;
				messages = messages;
				tick().then(() => scrollToBottom());
			}
		} else if (type === 'chat:completion') {
			// Completion event with choices (OpenAI format)
			if (activeResponseMessage && data?.choices) {
				const deltaContent = data.choices[0]?.delta?.content ?? '';
				
				if (deltaContent) {
					if (editEnabled) {
						if (!activeEnhancedContent) {
							activeEnhancedContent = {
								json: null,
								html: '',
								md: ''
							};
						}

						editing = true;
						streaming = true;

						activeEnhancedContent.md = (activeEnhancedContent.md || '') + deltaContent;
						activeEnhancedContent.html = marked.parse(activeEnhancedContent.md || '');

						if (!selectedContent || !selectedContent?.text) {
							note.data.content.md = activeEnhancedContent.md;
							note.data.content.html = activeEnhancedContent.html;
							note.data.content.json = null;
						}

						scrollToBottomHandler();

						activeResponseMessage.content = `<status title="${$i18n.t('Editing')}" done="false" />`;
						messages = messages;
					} else {
						activeResponseMessage.content += deltaContent;
						messages = messages;
					}

					tick().then(() => scrollToBottom());
				}

				// Check if done
				if (data.done) {
					if (editEnabled && activeEnhancedContent) {
						activeResponseMessage.content = `<status title="${$i18n.t('Edited')}" done="true" />`;

						if (selectedContent && selectedContent?.text && editor) {
							editor.commands.insertContentAt(
								{
									from: selectedContent.from,
									to: selectedContent.to
								},
								activeEnhancedContent.html || activeEnhancedContent.md || ''
							);

							selectedContent = null;
						}

						editing = false;
						streaming = false;
						onEdited();
					}

					activeResponseMessage.done = true;
					messages = messages;

					// Clear active state
					activeMessageId = null;
					activeResponseMessage = null;
					activeEnhancedContent = null;
					loading = false;
					stopResponseFlag = false;
				}
			}
		} else if (type === 'end' || type === 'error') {
			// Stream ended or error
			if (activeResponseMessage) {
				if (editEnabled && activeEnhancedContent) {
					activeResponseMessage.content = `<status title="${$i18n.t('Edited')}" done="true" />`;

					if (selectedContent && selectedContent?.text && editor) {
						editor.commands.insertContentAt(
							{
								from: selectedContent.from,
								to: selectedContent.to
							},
							activeEnhancedContent.html || activeEnhancedContent.md || ''
						);

						selectedContent = null;
					}

					editing = false;
					streaming = false;
					onEdited();
				}

				activeResponseMessage.done = true;
				messages = messages;
			}

			// Clear active state
			activeMessageId = null;
			activeResponseMessage = null;
			activeEnhancedContent = null;
			loading = false;
			stopResponseFlag = false;

			if (type === 'error') {
				toast.error(data?.detail || data?.error || 'An error occurred during streaming');
			}
		}
	};

	const chatCompletionHandler = async () => {
		if (selectedModelId === '') {
			toast.error($i18n.t('Please select a model.'));
			return;
		}

		const model = $models.find((model) => model.id === selectedModelId);
		if (!model) {
			selectedModelId = '';
			return;
		}

		let responseMessage;
		// Generate or reuse message ID for Socket.IO tracking
		let messageId;
		if (messages.at(-1)?.role === 'assistant') {
			responseMessage = messages.at(-1);
			// Reuse existing message ID if available
			messageId = responseMessage.id || uuidv4();
			responseMessage.id = messageId;
		} else {
			messageId = uuidv4();
			responseMessage = {
				id: messageId,
				role: 'assistant',
				content: '',
				done: false
			};
			messages.push(responseMessage);
			messages = messages;
		}

		await tick();
		scrollToBottom();

		stopResponseFlag = false;
		// Initialize enhanced content BEFORE setting active state
		let enhancedContent = {
			json: null,
			html: '',
			md: ''
		};

		// Set active message state for Socket.IO handler AFTER initializing enhancedContent
		activeMessageId = messageId;
		activeResponseMessage = responseMessage;
		activeEnhancedContent = enhancedContent;

		system = '';

		if (editEnabled) {
			system = `${DEFAULT_DOCUMENT_EDITOR_PROMPT}\n\n`;
		} else {
			system = `You are a helpful assistant. Please answer the user's questions based on the context provided.\n\n`;
		}

		system +=
			`<notes>${note?.data?.content?.md ?? ''}</notes>` +
			(files && files.length > 0
				? `\n<context>${files.map((file) => `${file.name}: ${file?.file?.data?.content ?? 'Could not extract content'}\n`).join('')}</context>`
				: '') +
			(selectedContent ? `\n<selection>${selectedContent?.text}</selection>` : '');

		const chatMessages = JSON.parse(
			JSON.stringify([
				{
					role: 'system',
					content: `${system}`
				},
				...messages
			])
		);

		// Generate a temporary chat_id for this note chat session
		// Use note.id as base to keep consistency within the same note
		const noteChatId = `note-${note?.id || 'temp'}-chat`;

		const [res, controller] = await chatCompletion(
			localStorage.token,
			{
				model: model.id,
				stream: true,
				messages: chatMessages,
				// Add Socket.IO metadata for real-time streaming
				session_id: $socket?.id,
				chat_id: noteChatId,
				id: messageId,
				model_item: $models.find((m) => m.id === model.id)
				// ...(files && files.length > 0 ? { files } : {}) // TODO: Decide whether to use native file handling or not
			},
			`${WEBUI_BASE_URL}/api`
		);

		await tick();
		scrollToBottom();

		// Check if response indicates Socket.IO streaming
		if (res && res.ok) {
			const contentType = res.headers.get('content-type');
			const isJson = contentType && contentType.includes('application/json');

			// If it's JSON, check if it's a Socket.IO streaming response
			if (isJson) {
				const jsonResponse = await res.json();
				if (jsonResponse.status === 'streaming' && jsonResponse.message?.includes('Socket.IO')) {
					console.log('Using Socket.IO streaming for note chat');
					// Socket.IO is handling the streaming, just wait for events
					// The handleChatEvent function will process the chunks
					return;
				}
			}

			// Fall back to traditional HTTP SSE streaming
			console.log('Using HTTP SSE streaming for note chat');
			let messageContent = '';

			const reader = res.body
				.pipeThrough(new TextDecoderStream())
				.pipeThrough(splitStream('\n'))
				.getReader();

			while (true) {
				const { value, done } = await reader.read();
				if (done || stopResponseFlag) {
					if (stopResponseFlag) {
						controller.abort('User: Stop Response');
					}

					if (editEnabled) {
						editing = false;
						streaming = false;
						onEdited();
					}

					// Clear active state
					activeMessageId = null;
					activeResponseMessage = null;
					activeEnhancedContent = null;

					break;
				}

				try {
					let lines = value.split('\n');

					for (const line of lines) {
						if (line !== '') {
							console.log(line);
							if (line === 'data: [DONE]') {
								if (editEnabled) {
									responseMessage.content = `<status title="${$i18n.t('Edited')}" done="true" />`;

									if (selectedContent && selectedContent?.text && editor) {
										editor.commands.insertContentAt(
											{
												from: selectedContent.from,
												to: selectedContent.to
											},
											enhancedContent.html || enhancedContent.md || ''
										);

										selectedContent = null;
									}
								}

								responseMessage.done = true;
								messages = messages;
							} else {
								let data = JSON.parse(line.replace(/^data: /, ''));
								console.log(data);

								let deltaContent = data.choices[0]?.delta?.content ?? '';
								if (responseMessage.content == '' && deltaContent == '\n') {
									continue;
								} else {
									if (editEnabled) {
										editing = true;
										streaming = true;

										enhancedContent.md = (enhancedContent.md || '') + deltaContent;
										enhancedContent.html = marked.parse(enhancedContent.md || '');

										if (!selectedContent || !selectedContent?.text) {
											note.data.content.md = enhancedContent.md;
											note.data.content.html = enhancedContent.html;
											note.data.content.json = null;
										}

										scrollToBottomHandler();

										responseMessage.content = `<status title="${$i18n.t('Editing')}" done="false" />`;
										messages = messages;
									} else {
										messageContent += deltaContent;

										responseMessage.content = messageContent;
										messages = messages;
									}

									await tick();
								}
							}
						}
					}
				} catch (error) {
					console.log(error);
				}

				scrollToBottom();
			}
		}
	};

	const submitHandler = async (e) => {
		const { content, data } = e;
		if (selectedModelId && content) {
			messages.push({
				role: 'user',
				content: content
			});
			messages = messages;

			await tick();
			scrollToBottom();

			loading = true;
			await chatCompletionHandler();
			messages = messages.map((message) => {
				message.done = true;
				return message;
			});

			loading = false;
			stopResponseFlag = false;
		}
	};

	onMount(async () => {
		editEnabled = localStorage.getItem('noteEditEnabled') === 'true';

		// Listen for Socket.IO chat events
		$socket?.on('chat-events', handleChatEvent);

		loaded = true;

		await tick();
		scrollToBottom();

		return () => {
			// Cleanup Socket.IO listener
			$socket?.off('chat-events', handleChatEvent);
		};
	});
</script>

<div class="flex items-center mb-1.5 pt-1.5 px-2.5">
	<div class="flex items-center mr-1">
		<button
			class="p-0.5 bg-transparent transition rounded-lg"
			on:click={() => {
				show = !show;
			}}
		>
			<XMark className="size-5" strokeWidth="2.5" />
		</button>
	</div>

	<div class=" font-medium text-base flex items-center gap-1">
		<div>
			{$i18n.t('Chat')}
		</div>

		<div>
			<Tooltip
				content={$i18n.t(
					'This feature is experimental and may be modified or discontinued without notice.'
				)}
				position="top"
				className="inline-block"
			>
				<span class="text-gray-500 text-sm">({$i18n.t('Experimental')})</span>
			</Tooltip>
		</div>
	</div>
</div>

<div class="flex flex-col items-center flex-1 @container px-2.5">
	<div class=" flex flex-col justify-between w-full overflow-y-auto h-full">
		<div class="mx-auto w-full md:px-0 h-full relative">
			<div class=" flex flex-col h-full">
				<div
					class=" pb-2.5 flex flex-col justify-between w-full flex-auto overflow-auto h-0 scrollbar-hidden"
					id="messages-container"
					bind:this={messagesContainerElement}
					on:scroll={onScroll}
				>
					<div class=" h-full w-full flex flex-col">
						<div class="flex-1 p-1">
							<Messages bind:messages {onInsert} />
						</div>
					</div>
				</div>

				<div class=" pb-[1rem]">
					{#if selectedContent}
						<div class="text-xs rounded-xl px-3.5 py-3 w-full markdown-prose-xs">
							<blockquote>
								<div class=" line-clamp-3">
									{selectedContent?.text}
								</div>
							</blockquote>
						</div>
					{/if}

					<MessageInput
						bind:chatInputElement
						acceptFiles={false}
						inputLoading={loading}
						showFormattingToolbar={false}
						onSubmit={submitHandler}
						{onStop}
					>
						<div slot="menu" class="flex items-center justify-between gap-2 w-full pr-1">
							<div>
								<Tooltip content={$i18n.t('Edit')} placement="top">
									<button
										on:click|preventDefault={() => {
											editEnabled = !editEnabled;

											localStorage.setItem('noteEditEnabled', editEnabled ? 'true' : 'false');
										}}
										disabled={streaming || loading}
										type="button"
										class="px-2 @xl:px-2.5 py-2 flex gap-1.5 items-center text-sm rounded-full transition-colors duration-300 focus:outline-hidden max-w-full overflow-hidden hover:bg-gray-50 dark:hover:bg-gray-800 {editEnabled
											? ' text-sky-500 dark:text-sky-300 bg-sky-50 dark:bg-sky-200/5'
											: 'bg-transparent text-gray-600 dark:text-gray-300 '} disabled:opacity-50 disabled:pointer-events-none"
									>
										<PencilSquare className="size-4" strokeWidth="1.75" />
										<span
											class="block whitespace-nowrap overflow-hidden text-ellipsis leading-none pr-0.5"
											>{$i18n.t('Edit')}</span
										>
									</button>
								</Tooltip>
							</div>

							<Tooltip content={selectedModelId}>
								<select
									class=" bg-transparent rounded-lg py-1 px-2 -mx-0.5 text-sm outline-hidden w-full text-right pr-5"
									bind:value={selectedModelId}
								>
									{#each $models.filter((model) => !(model?.info?.meta?.hidden ?? false)) as model}
										<option value={model.id} class="bg-gray-50 dark:bg-gray-700"
											>{model.name}</option
										>
									{/each}
								</select>
							</Tooltip>
						</div>
					</MessageInput>
				</div>
			</div>
		</div>
	</div>
</div>
