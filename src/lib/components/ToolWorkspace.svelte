<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { cubicIn, cubicOut } from 'svelte/easing';
	import { fade, slide } from 'svelte/transition';
	import {
		IconArrowRight,
		IconArrowsSort,
		IconCamera,
		IconCheck,
		IconDownload,
		IconFilePlus,
		IconLoader2
	} from '@tabler/icons-svelte-runes';
	import {
		isToolSupported,
		nextTools,
		toolCategoryColor,
		type CatalogTool
	} from '$lib/tool-catalog';
	import type { SplitRange } from '$lib/split-ranges';
	import type { OrganizePage, SplitOptions } from '$lib/pdf/types';
	import { pageKey } from '$lib/pdf/sources';
	import { getWorkspace } from '$lib/workspace.svelte';
	import {
		pdfProtection,
		processProtectPdf,
		unlockPdf,
		convertToPdfA,
		processCompressPdf,
		processImagesToPdf,
		processPrint,
		processOrganizePdf,
		processPdfToImages,
		processPageNumbers,
		processPdfs,
		processSplitPdf,
		processWatermark,
		processCrop,
		processSign,
		processFlatten,
		processRedact,
		processAnnotate,
		processEdit,
		processFillForm,
		processOcr,
		processScan,
		scanPhoto,
		redactionText,
		pdfCondition
	} from '$lib/pdf/processor';
	import { repairPdf } from '$lib/pdf/repair';
	import type {
		CoveredPage,
		CropArea,
		CropOptions,
		ImagePdfOptions,
		PageGlyphs,
		PdfImageOptions,
		PicturedPage,
		PdfOutput,
		Protection,
		RedactMark
	} from '$lib/pdf/types';
	import { DownloadJob } from '$lib/pdf/download-job.svelte';
	import { parsePageRange } from '$lib/pdf/page-range';
	import {
		officeOperation,
		officeTools,
		printOperation,
		toolInput,
		formatName,
		inputAccept,
		acceptsFile
	} from '$lib/pdf/office-conversion';
	import { layoutHtml, readHtml, type PrintSize } from '$lib/pdf/html-print';
	import { markdownDocument, type Typeface } from '$lib/pdf/markdown-print';
	import { processOfficeFile } from '$lib/pdf/office-processor';
	import PdfDropzone from './PdfDropzone.svelte';
	import DocumentCards from './DocumentCards.svelte';
	import CompressSettings from './CompressSettings.svelte';
	import SplitSettings from './SplitSettings.svelte';
	import SplitViewer from './SplitViewer.svelte';
	import OrganizeViewer from './OrganizeViewer.svelte';
	import PageSelectSettings from './PageSelectSettings.svelte';
	import PdfToImageSettings from './PdfToImageSettings.svelte';
	import ImageToPdfSettings from './ImageToPdfSettings.svelte';
	import OutputFilename from './OutputFilename.svelte';
	import AdvancedOptions from './AdvancedOptions.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';
	import PdfToImageAdvanced from './PdfToImageAdvanced.svelte';
	import PageRangeField from './PageRangeField.svelte';
	import CopyButton from './CopyButton.svelte';
	import ContinueWith from './ContinueWith.svelte';
	import PdfASettings from './PdfASettings.svelte';
	import PasswordInput from './PasswordInput.svelte';
	import StampPreview from './StampPreview.svelte';
	import PageScroller from './PageScroller.svelte';
	import PrintSettings from './PrintSettings.svelte';
	import PrintPreview from './PrintPreview.svelte';
	import StampOverlay from './StampOverlay.svelte';
	import PageNumberSettings from './PageNumberSettings.svelte';
	import WatermarkSettings from './WatermarkSettings.svelte';
	import StampTextSettings from './StampTextSettings.svelte';
	import StampAdvanced from './StampAdvanced.svelte';
	import CropOverlay from './CropOverlay.svelte';
	import CropSettings from './CropSettings.svelte';
	import SignOverlay from './SignOverlay.svelte';
	import SignSettings from './SignSettings.svelte';
	import FlattenOverlay from './FlattenOverlay.svelte';
	import FlattenSettings from './FlattenSettings.svelte';
	import RedactOverlay from './RedactOverlay.svelte';
	import RedactSettings from './RedactSettings.svelte';
	import AnnotateOverlay from './AnnotateOverlay.svelte';
	import AnnotateSettings from './AnnotateSettings.svelte';
	import FormOverlay from './FormOverlay.svelte';
	import FormSettings from './FormSettings.svelte';
	import CompareViewer from './CompareViewer.svelte';
	import CompareSettings from './CompareSettings.svelte';
	import OcrSettings from './OcrSettings.svelte';
	import OcrOverlay from './OcrOverlay.svelte';
	import TranslateSettings from './TranslateSettings.svelte';
	import TranslateOverlay from './TranslateOverlay.svelte';
	import { TranslateDocument } from '$lib/pdf/translate-document.svelte';
	import { translateLanguageName } from '$lib/pdf/translate.svelte';
	import ScanSettings from './ScanSettings.svelte';
	import ScanEditor from './ScanEditor.svelte';
	import ScanCamera from './ScanCamera.svelte';
	import { FULL_PHOTO, ScanPhotos, pendingTurn } from '$lib/pdf/scan.svelte';
	import {
		OcrModels,
		OcrReader,
		languageName,
		ocrText,
		readPages,
		type OcrPageState,
		type OcrWord
	} from '$lib/pdf/ocr.svelte';
	import { compareText, hasText, markup, type CompareSide } from '$lib/pdf/compare-text';
	import {
		blankValues,
		changedFields,
		formFills,
		readFormFields,
		undrawableField,
		formTexts,
		type FormFields,
		type FormValue
	} from '$lib/pdf/form-fields';
	import { AnnotateEditor } from '$lib/pdf/annotate-editor.svelte';
	import { covered, findText, glyphBox, glyphText, isBlank } from '$lib/pdf/redact-text';
	import { flattenMarks, redactRemovals, type AnnotationMark } from '$lib/pdf/annotations';
	import type { PDFDocumentProxy } from 'pdfjs-dist';
	import { FULL_PAGE, contentBounds, cropSize, isFullPage, padArea } from '$lib/pdf/crop-area';
	import { undrawable, type FontFamily } from '$lib/pdf/standard-fonts';
	import type { PreviewPage, StampImage, StampMark, StampPosition } from '$lib/pdf/stamp-layout';
	import { rangeCollapse, rangeReveal } from '$lib/motion/range';
	let { tool }: { tool: CatalogTool } = $props();
	const workspace = getWorkspace();
	const supported = $derived(isToolSupported(tool.id));
	const isMerge = $derived(tool.id === 'merge');
	const isSplit = $derived(tool.id === 'split');
	const isOrganize = $derived(tool.id === 'organize');
	const isExtract = $derived(tool.id === 'extract');
	const isRemove = $derived(tool.id === 'remove');
	const isRotate = $derived(tool.id === 'rotate');
	const isPageTool = $derived(isOrganize || isExtract || isRemove || isRotate);
	const isCompress = $derived(tool.id === 'compress');
	const isProtect = $derived(tool.id === 'protect');
	const isUnlock = $derived(tool.id === 'unlock');
	const isPdfA = $derived(tool.id === 'pdf-to-pdfa');
	const isRepair = $derived(tool.id === 'repair');
	const isPageNumbers = $derived(tool.id === 'page-numbers');
	const isWatermark = $derived(tool.id === 'watermark');
	const isStamp = $derived(isPageNumbers || isWatermark);
	const isCrop = $derived(tool.id === 'crop');
	const isSign = $derived(tool.id === 'sign');
	const isFlatten = $derived(tool.id === 'flatten');
	const isRedact = $derived(tool.id === 'redact');
	const isAnnotate = $derived(tool.id === 'annotate');
	const isForms = $derived(tool.id === 'forms');
	const isEdit = $derived(tool.id === 'edit');
	const isCompare = $derived(tool.id === 'compare');
	const isOcr = $derived(tool.id === 'ocr');
	const isTranslate = $derived(tool.id === 'translate');
	const isScan = $derived(tool.id === 'scan-to-pdf');
	// Tools that show the pages and draw their result over them.
	const isPagePreview = $derived(
		isStamp ||
			isCrop ||
			isSign ||
			isFlatten ||
			isRedact ||
			isAnnotate ||
			isForms ||
			isEdit ||
			isOcr ||
			isTranslate
	);
	const isPdfToImage = $derived(
		tool.id === 'pdf-to-jpg' ||
			tool.id === 'pdf-to-png' ||
			tool.id === 'pdf-to-image' ||
			tool.id === 'pdf-to-images'
	);
	const isImageToPdf = $derived(
		tool.id === 'jpg-to-pdf' ||
			tool.id === 'png-to-pdf' ||
			tool.id === 'image-to-pdf' ||
			tool.id === 'images-to-pdf'
	);
	const officeTool = $derived(officeOperation(tool.id));
	const printTool = $derived(printOperation(tool.id));
	const isMarkdown = $derived(tool.id === 'pdf-to-markdown');
	const inputType = $derived(toolInput(tool.id) ?? (isImageToPdf || isScan ? 'image' : 'pdf'));
	workspace.use(untrack(() => inputType));
	workspace.receive();
	const accent = $derived(toolCategoryColor(tool.id));
	const buttonColor = $derived(
		(
			{
				'text-brand': 'bg-brand',
				'text-convert': 'bg-convert',
				'text-compress': 'bg-compress',
				'text-merge': 'bg-merge',
				'text-split': 'bg-split'
			} as Record<string, string>
		)[accent] ?? 'bg-brand'
	);
	const cardMode = $derived(
		isMerge
			? 'merge'
			: isCompress
				? 'compress'
				: isImageToPdf
					? 'image'
					: isScan
						? 'scan'
						: 'single'
	);
	let input = $state<HTMLInputElement>();
	let dragged = $state<File | null>(null);
	let keyboardPicked = $state<File | null>(null);
	let reducedMotion = $state(false);
	const currentFile = $derived(workspace.files[0]);
	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const update = () => {
			reducedMotion = preference.matches;
		};
		update();
		preference.addEventListener('change', update);
		return () => preference.removeEventListener('change', update);
	});
	let pageCount = $state(0);
	let organizePages = $state<OrganizePage[]>([]);
	let selectedPages = $state<string[]>([]);
	let extractOutput = $state<'pdf' | 'separate' | 'images'>('pdf');
	let mergeBookmarks = $state(false);
	let protectPassword = $state('');
	let protectConfirm = $state('');
	let protectOwner = $state('');
	let allowPrinting = $state(true);
	let allowCopying = $state(true);
	let allowEditing = $state(true);
	let pdfaPart = $state<2 | 3>(2);
	// What the engine makes of the file as chosen: its page count, -1 when it
	// waits for a password, 0 when it cannot open it.
	let repairCondition = $state<number | 'checking'>('checking');
	// What the last repair fixed, or null before one has run.
	let repairFindings = $state.raw<string[] | null>(null);
	let unlockProtection = $state<Protection | 'checking'>('checking');
	let unlockPassword = $state('');
	// The password the engine last rejected; the error clears once it is edited.
	let rejectedPassword = $state('');
	const unlockWrong = $derived(rejectedPassword !== '' && rejectedPassword === unlockPassword);
	let organizeViewer = $state<{ addBlankPage: () => Promise<void> }>();
	let officeStage = $state<'loading' | 'converting'>('loading');
	let printSize = $state<PrintSize>('a4');
	let printLandscape = $state(false);
	let printMargin = $state(36);
	let printTypeface = $state<Typeface>('sans');
	// The PDF the options make: the preview, and what downloads as it is.
	let printOutput = $state.raw<PdfOutput | null>(null);
	let printPreview = $state.raw<File | null>(null);
	let printSource: File | undefined;
	let printError = $state('');
	let printBusy = $state(false);
	// Images the document only links to, which the PDF leaves out.
	let printSkipped = $state(0);
	let markdownPages = $state('');
	let markdownImages = $state(false);
	let markdownText = $state('');
	let extractImageFormat = $state<'jpg' | 'png'>('jpg');
	let extractDpi = $state(150);
	let splitRanges = $state<SplitRange[]>([{ id: 0, from: 1, to: 1 }]);
	let splitMode = $state<'ranges' | 'fixed'>('ranges');
	let splitInterval = $state(1);
	let splitCombine = $state(false);
	let compressLevel = $state<'light' | 'balanced' | 'strong'>('balanced');
	let compressRemoveMetadata = $state(false);
	let compressRemoveThumbnails = $state(false);
	let numberPosition = $state<StampPosition>(7);
	let numberTemplate = $state('{n}');
	let numberFirst = $state(1);
	let numberPages = $state('');
	let numberFamily = $state<FontFamily>('helvetica');
	let numberBold = $state(false);
	let numberSize = $state(11);
	let numberColor = $state('#000000');
	let numberMargin = $state(36);
	let watermarkKind = $state<'text' | 'image'>('text');
	let watermarkText = $state('CONFIDENTIAL');
	let watermarkFamily = $state<FontFamily>('helvetica');
	let watermarkBold = $state(true);
	let watermarkSize = $state(56);
	let watermarkColor = $state('#6b7280');
	let watermarkImage = $state<StampImage | null>(null);
	let watermarkImageWidth = $state(40);
	let watermarkPosition = $state<StampPosition>(4);
	let watermarkTile = $state(false);
	let watermarkRotation = $state(45);
	let watermarkOpacity = $state(40);
	let watermarkBehind = $state(false);
	let watermarkMargin = $state(36);
	let watermarkPages = $state('');
	let watermarkTooDense = $state(false);
	let cropMode = $state<'manual' | 'auto'>('manual');
	let cropArea = $state.raw<CropArea>(FULL_PAGE);
	let cropPadding = $state(9);
	let cropPages = $state('');
	// What each page drew when the preview rendered it, by page number, and
	// the page last drawn. Fit content and the Content preview use these; the
	// worker finds the bounds again for every page it crops.
	let cropBounds = $state.raw<Record<number, CropArea | null>>({});
	let cropPage = $state.raw<PreviewPage | null>(null);
	let signImage = $state<StampImage | null>(null);
	let signScope = $state<'one' | 'all' | 'custom'>('one');
	let signRange = $state('');
	let signPage = $state(1);
	// Bottom right, where most documents leave room for a signature.
	const SIGN_PLACE: [number, number, number] = [0.6, 0.8, 0.28];
	let signPlace = $state.raw<[number, number, number]>(SIGN_PLACE);
	let flattenFormsOnly = $state(false);
	// What each page will have drawn into it, read from the preview's copy.
	let flattenScan = $state.raw<AnnotationMark[][] | 'checking' | 'unknown'>('checking');
	// Annotations the engine left as they were in the last result.
	let flattenKept = $state(0);
	let redactMarks = $state.raw<RedactMark[]>([]);
	// Earlier states of the marks, newest last, for Undo.
	let redactHistory = $state.raw<RedactMark[][]>([]);
	let redactSelected = $state<number | null>(null);
	let redactNextId = 0;
	let redactFill = $state<'black' | 'white'>('black');
	let redactMetadata = $state(false);
	let redactAsImages = $state(false);
	let redactQuery = $state('');
	// The search hit being looked at, or -1.
	let redactMatch = $state(-1);
	const annotate = new AnnotateEditor();
	const edit = new AnnotateEditor('edit');
	// Pages where the last result painted over what it could not take out.
	let editCovered = $state.raw<CoveredPage[]>([]);
	let formFields = $state.raw<FormFields | 'reading' | 'unknown'>('reading');
	let formValues = $state.raw<Record<string, FormValue>>({});
	// The canvases pdf.js drew each page's check boxes and other separately
	// drawn annotations on, by page number.
	let formCanvases = $state.raw<
		Record<number, Map<string, HTMLCanvasElement | HTMLCanvasElement[]>>
	>({});
	let formFlatten = $state(false);
	// Fields the engine left fillable in the last flattened result.
	let formKept = $state(0);
	let compareMode = $state<'text' | 'visual'>('text');
	let compareHighlight = $state(true);
	// The change being looked at, or -1.
	let compareCurrent = $state(-1);
	// Each file's text as the engine reads it, kept by file so swapping the two
	// reads nothing again.
	const compareGlyphs = new SvelteMap<File, PageGlyphs[] | 'reading' | 'unknown'>();
	const compareOriginal = $derived(isCompare ? workspace.files[0] : undefined);
	const compareChanged = $derived(isCompare ? workspace.files[1] : undefined);
	const compareTexts = $derived(
		compareOriginal && compareChanged
			? [compareOriginal, compareChanged].map((file) => compareGlyphs.get(file) ?? 'reading')
			: null
	);
	const compareTextState = $derived(
		!compareTexts
			? 'waiting'
			: compareTexts.includes('reading')
				? 'reading'
				: compareTexts.includes('unknown')
					? 'unknown'
					: (compareTexts as PageGlyphs[][]).some(hasText)
						? 'ready'
						: 'none'
	);
	const comparison = $derived(
		compareTexts && compareTextState === 'ready'
			? compareText(compareTexts[0] as PageGlyphs[], compareTexts[1] as PageGlyphs[])
			: null
	);
	const compareValid = $derived(!!comparison && comparison.changes.length > 0);
	$effect(() => {
		void comparison;
		compareCurrent = -1;
	});
	async function readCompareText(file: File) {
		const known = compareGlyphs.get(file);
		if (known && known !== 'unknown') return;
		compareGlyphs.set(file, 'reading');
		let read: PageGlyphs[] | 'unknown';
		try {
			read = await redactionText(file, workspace.passwordFor(file));
		} catch {
			read = 'unknown';
		}
		if (workspace.files.includes(file)) compareGlyphs.set(file, read);
	}
	/// Files for one side, or with no side for whichever is empty, else the
	/// changed one. Two files given at once fill both sides.
	function placeCompare(side: CompareSide | null, incoming: FileList) {
		const pdfs = Array.from(incoming).filter((file) => acceptsFile(file, 'pdf'));
		if (!pdfs.length) {
			workspace.error = 'Please choose a PDF file.';
			return;
		}
		workspace.error = '';
		let [original, changed] = [workspace.files[0], workspace.files[1]];
		if (side === 'original') [original, changed] = [pdfs[0], changed ?? pdfs[1]];
		else if (side === 'changed') [original, changed] = [original ?? pdfs[1], pdfs[0]];
		else if (pdfs.length > 1) [original, changed] = [pdfs[0], pdfs[1]];
		else if (!original) original = pdfs[0];
		else changed = pdfs[0];
		workspace.files = [original, changed].filter((file): file is File => !!file);
	}
	// What the engine reads on each page, for finding text and picking words.
	let pageText = $state.raw<PageGlyphs[] | 'reading' | 'unknown'>('reading');
	const ocrModels = new OcrModels();
	const ocrReader = new OcrReader();
	let ocrLanguages = $state(['eng']);
	let ocrPages = $state('');
	let ocrSkipText = $state(true);
	// Words read so far, by languages and page, so a run picks up where a
	// cancelled one stopped and changing the pages reads only new ones.
	const ocrRead = new SvelteMap<string, OcrWord[]>();
	// Pages the running job is reading.
	let ocrReading = $state.raw<Record<number, OcrPageState>>({});
	let ocrStage = $state<'models' | 'starting' | 'reading' | 'writing'>('models');
	let ocrDone = $state(0);
	let ocrTotal = $state(0);
	// Pages of the last result left alone for having text, and read to nothing.
	let ocrSkipped = $state.raw<number[]>([]);
	let ocrEmpty = $state.raw<number[]>([]);
	// The result's text as the engine reads it back, for Find text.
	let ocrFound = $state.raw<PageGlyphs[] | null>(null);
	let ocrQuery = $state('');
	let ocrMatch = $state(-1);
	let scroller = $state<{ reveal: (page: number, at?: number) => void }>();
	// A page this many characters long has text of its own. Fewer is a stamp,
	// a page number or a Bates number on a scan, which still wants reading;
	// what it does say is not read twice, since words over text are dropped.
	const OCR_TEXT_GLYPHS = 32;
	const ocrKey = $derived(ocrLanguages.join('+'));
	const ocrHasText = $derived.by(() => {
		if (typeof pageText === 'string') return [];
		return pageText.map((page) => {
			let count = 0;
			for (let index = 0; index < page.ends.length; index++)
				if (!isBlank(glyphText(page, index)) && ++count >= OCR_TEXT_GLYPHS) return true;
			return false;
		});
	});
	const ocrScope = $derived(
		ocrPages.trim()
			? (parsePageRange(ocrPages, pageCount) ?? [])
			: Array.from({ length: pageCount }, (_, index) => index + 1)
	);
	const ocrPagesInvalid = $derived(
		ocrPages.trim() !== '' && pageCount > 0 && ocrScope.length === 0
	);
	const ocrTargets = $derived(ocrScope.filter((page) => !(ocrSkipText && ocrHasText[page - 1])));
	const ocrValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			pageText !== 'reading' &&
			ocrLanguages.length > 0 &&
			!ocrPagesInvalid &&
			ocrTargets.length > 0
	);
	const ocrSignature = $derived(JSON.stringify([ocrLanguages, ocrPages, ocrSkipText]));
	function ocrState(page: number): OcrPageState | 'skipped' | undefined {
		const reading = ocrReading[page];
		if (reading) return reading;
		const words = ocrRead.get(`${ocrKey}|${page}`);
		if (words) return { status: 'done', words };
		return ocrSkipText && ocrHasText[page - 1] && ocrScope.includes(page) ? 'skipped' : undefined;
	}
	const ocrMatches = $derived(ocrFound ? findText(ocrFound, ocrQuery) : []);
	const ocrCurrent = $derived(ocrMatches[ocrMatch]);
	$effect(() => {
		void ocrQuery;
		void ocrFound;
		ocrMatch = -1;
	});
	function stepOcrMatch(offset: number) {
		const count = ocrMatches.length;
		if (!count) return;
		ocrMatch = ocrMatch < 0 ? (offset > 0 ? 0 : count - 1) : (ocrMatch + offset + count) % count;
		const { page, boxes } = ocrMatches[ocrMatch];
		scroller?.reveal(page, boxes[0][1]);
	}
	// The languages' models come down as soon as the tool opens or one is
	// picked, and a reader starts warming up, so the first page reads at once.
	$effect(() => {
		if (!isOcr) return;
		const languages = [...ocrLanguages];
		let cancelled = false;
		Promise.all(languages.map((code) => ocrModels.ensure(code)))
			.then(() => {
				if (!cancelled && !untrack(() => processing)) return ocrReader.prepare(languages, 1);
			})
			.catch(() => {});
		return () => {
			cancelled = true;
		};
	});
	function modelStatus(languages: string[]) {
		for (const code of languages) {
			const state = ocrModels.states.get(code);
			if (state?.status === 'loading' && state.total)
				return `Downloading ${languageName(code)} ${Math.min(99, Math.floor((state.received / state.total) * 100))}%`;
		}
		return 'Loading languages...';
	}
	const ocrStatus = $derived.by(() => {
		if (ocrStage === 'models') return modelStatus(ocrLanguages);
		if (ocrStage === 'starting') return 'Starting...';
		if (ocrStage === 'writing') return 'Writing text...';
		return `Reading ${ocrDone} of ${ocrTotal}...`;
	});
	const translate = new TranslateDocument();
	// Pages of the last result with nothing to translate, and painted over.
	let translateEmpty = $state.raw<number[]>([]);
	let translateCovered = $state.raw<CoveredPage[]>([]);
	const translateValid = $derived(
		!!currentFile &&
			translate.pageCount > 0 &&
			translate.pairs.length > 0 &&
			!translate.pagesInvalid &&
			translate.targets.length > 0
	);
	// The document's text and page sizes are read separately; the tool
	// needs both.
	let translateSizes = $state.raw<{ width: number; height: number }[]>([]);
	async function readPageSizes(pdf: PDFDocumentProxy) {
		const sizes = await Promise.all(
			Array.from({ length: pdf.numPages }, async (_, index) => {
				const { width, height } = (await pdf.getPage(index + 1)).getViewport({ scale: 1 });
				return { width, height };
			})
		);
		if (pdf === untrack(() => translatePdfProxy)) translateSizes = sizes;
	}
	let translatePdfProxy: PDFDocumentProxy | undefined;
	$effect(() => {
		if (!isTranslate || typeof pageText === 'string' || !translateSizes.length) return;
		const [glyphs, sizes] = [pageText, translateSizes];
		if (glyphs.length === sizes.length) untrack(() => void translate.read(glyphs, sizes));
	});
	// The models come down as soon as there is something to translate.
	$effect(() => {
		if (!isTranslate || !translate.hasText) return;
		void translate.pairs;
		untrack(() => translate.prefetch());
	});
	const translateStatus = $derived.by(() => {
		if (translate.stage === 'models') {
			for (const pair of translate.pairs) {
				const state = translate.models.states.get(pair);
				if (state?.status === 'loading' && state.total) {
					const language = pair.startsWith('en-') ? pair.slice(3) : pair.slice(0, -3);
					const name = translateLanguageName(language === 'hbs' ? translate.from : language);
					return `Downloading ${name} ${Math.min(99, Math.floor((state.received / state.total) * 100))}%`;
				}
			}
			return 'Loading languages...';
		}
		if (translate.stage === 'starting') return 'Starting...';
		if (translate.stage === 'writing') return 'Writing pages...';
		return `Translating ${translate.done} of ${translate.total}...`;
	});
	async function translatePdf() {
		if (processing || !translateValid || !currentFile) return;
		const file = currentFile;
		const password = workspace.passwordFor(file);
		translateEmpty = [];
		translateCovered = [];
		await job.run(
			async (signal) => {
				const { output, covered } = await translate.run(file, password, previewPage, signal);
				translateEmpty = [...translate.empty];
				translateCovered = covered;
				return output;
			},
			'Could not translate this PDF.',
			() => downloadLink?.click()
		);
	}
	const scan = new ScanPhotos();
	// The photo whose corners are open on the canvas.
	let scanEditing = $state<File | null>(null);
	let scanCamera = $state(false);
	const cameraAvailable =
		typeof navigator !== 'undefined' && !!navigator.mediaDevices?.getUserMedia;
	let scanPaper = $state<'a4' | 'letter' | 'fit'>('a4');
	let scanSearchable = $state(false);
	let scanLanguages = $state(['eng']);
	let scanStage = $state<'scanning' | 'writing' | 'models' | 'starting' | 'reading' | 'text'>(
		'scanning'
	);
	let scanDone = $state(0);
	let scanTotal = $state(0);
	// Letter where it is the usual paper, A4 everywhere else.
	function usesLetter() {
		const region = new Intl.Locale(navigator.language).maximize().region ?? '';
		return ['US', 'CA', 'MX', 'PH', 'CL', 'CO', 'VE', 'GT', 'PR'].includes(region);
	}
	onMount(() => {
		if (usesLetter()) scanPaper = 'letter';
	});
	$effect(() => {
		if (!isScan) return;
		const files = workspace.files;
		untrack(() => {
			scan.sync(files);
			if (scanEditing && !files.includes(scanEditing)) scanEditing = null;
		});
	});
	$effect(() => {
		scan.focus = scanEditing;
	});
	$effect(() => {
		if (!isScan || !scanSearchable) return;
		for (const code of scanLanguages) void ocrModels.ensure(code).catch(() => {});
	});
	const scanValid = $derived(
		workspace.files.length > 0 &&
			workspace.files.every((file) => scan.get(file)?.status === 'ready') &&
			(!scanSearchable || scanLanguages.length > 0)
	);
	const scanSignature = $derived(
		isScan
			? JSON.stringify([
					workspace.files.map((file) => scan.page(file)),
					scan.look,
					scanPaper,
					scanSearchable,
					scanLanguages
				])
			: ''
	);
	const scanStatus = $derived(
		scanStage === 'scanning'
			? `Scanning ${scanDone} of ${scanTotal}...`
			: scanStage === 'writing'
				? 'Writing PDF...'
				: scanStage === 'models'
					? modelStatus(scanLanguages)
					: scanStage === 'starting'
						? 'Starting...'
						: scanStage === 'reading'
							? `Reading ${scanDone} of ${scanTotal}...`
							: 'Writing text...'
	);
	function stepScanEditor(offset: number) {
		if (!scanEditing) return;
		const next = workspace.files[workspace.files.indexOf(scanEditing) + offset];
		if (next) scanEditing = next;
	}
	async function scanToPdf() {
		if (processing || !scanValid) return;
		const files = [...workspace.files];
		const pages = files.map((file) => scan.page(file));
		const look = scan.look;
		const paper =
			scanPaper === 'a4'
				? { width: 595.28, height: 841.89 }
				: scanPaper === 'letter'
					? { width: 612, height: 792 }
					: { width: usesLetter() ? 612 : 595.28, height: 0 };
		const languages = scanSearchable ? [...scanLanguages] : [];
		scanEditing = null;
		await job.run(
			async (signal) => {
				const stop = () => ocrReader.destroy();
				signal.addEventListener('abort', stop, { once: true });
				try {
					scanStage = 'scanning';
					scanDone = 0;
					scanTotal = files.length;
					const images: Uint8Array[] = [];
					for (const [index, file] of files.entries()) {
						// One bit a pixel affords more pixels, and thin strokes need them.
						const side = look === 'bw' ? 3300 : 2400;
						images.push((await scanPhoto(file, pages[index], look, side, signal)).bytes);
						scanDone++;
					}
					scanStage = 'writing';
					let output = await processScan(images, paper, signal);
					if (!languages.length) return output;
					scanStage = 'models';
					await Promise.all(languages.map((code) => ocrModels.ensure(code)));
					scanStage = 'starting';
					const cores = navigator.hardwareConcurrency || 2;
					const lanes = Math.min(files.length, cores >= 8 ? 3 : cores >= 4 ? 2 : 1);
					await ocrReader.prepare(languages, lanes);
					if (signal.aborted) throw new DOMException('Cancelled', 'AbortError');
					scanStage = 'reading';
					scanDone = 0;
					const pdf = new File([output.bytes.slice().buffer], 'scan.pdf', {
						type: 'application/pdf'
					});
					const words: OcrWord[] = [];
					await readPages(
						pdf,
						'',
						files.map((_, index) => index + 1),
						ocrReader,
						(_, state) => {
							if (state.status !== 'done' || signal.aborted) return;
							words.push(...state.words);
							scanDone++;
						},
						signal
					);
					if (signal.aborted) throw new DOMException('Cancelled', 'AbortError');
					if (words.length) {
						scanStage = 'text';
						output = await processOcr(pdf, '', words.map(ocrText), signal);
					}
					return output;
				} finally {
					signal.removeEventListener('abort', stop);
				}
			},
			'Could not scan these photos.',
			() => downloadLink?.click()
		);
		void scan.refresh();
	}

	function pageList(numbers: number[]) {
		return numbers.length === 1
			? `page ${numbers[0]}`
			: `pages ${numbers.slice(0, -1).join(', ')} and ${numbers.at(-1)}`;
	}
	// Said once, after the result.
	const ocrNotice = $derived(
		[
			ocrSkipped.length
				? `${ocrSkipped.length === 1 ? 'Page' : 'Pages'} ${pageList(ocrSkipped).replace(/^pages? /, '')} already ${ocrSkipped.length === 1 ? 'has' : 'have'} text and ${ocrSkipped.length === 1 ? 'was' : 'were'} left as ${ocrSkipped.length === 1 ? 'it was' : 'they were'}.`
				: '',
			ocrEmpty.length ? `No text was found on ${pageList(ocrEmpty)}.` : ''
		]
			.filter(Boolean)
			.join(' ')
	);
	/// Words over text the page already has are left out, so nothing reads twice.
	function freshWords(words: OcrWord[], page: PageGlyphs | undefined) {
		if (!page?.ends.length) return words;
		const boxes: CropArea[] = [];
		for (let index = 0; index < page.ends.length; index++)
			if (!isBlank(glyphText(page, index))) boxes.push(glyphBox(page, index));
		return words.filter((word) => !covered(word.box, boxes, 0.5));
	}
	async function readText() {
		if (processing || !ocrValid || !currentFile || typeof pageText === 'string') return;
		const file = currentFile;
		const password = workspace.passwordFor(file);
		const languages = [...ocrLanguages];
		const key = languages.join('+');
		const targets = [...ocrTargets];
		const skipped = ocrScope.filter((page) => !targets.includes(page));
		const glyphs = pageText;
		ocrFound = null;
		ocrQuery = '';
		ocrSkipped = [];
		ocrEmpty = [];
		let written: Uint8Array | undefined;
		await job.run(
			async (signal) => {
				const stop = () => ocrReader.destroy();
				signal.addEventListener('abort', stop, { once: true });
				try {
					ocrStage = 'models';
					await Promise.all(languages.map((code) => ocrModels.ensure(code)));
					const missing = targets.filter((page) => !ocrRead.has(`${key}|${page}`));
					ocrTotal = targets.length;
					ocrDone = targets.length - missing.length;
					if (missing.length) {
						ocrStage = 'starting';
						const cores = navigator.hardwareConcurrency || 2;
						const lanes = Math.min(missing.length, cores >= 8 ? 3 : cores >= 4 ? 2 : 1);
						await ocrReader.prepare(languages, lanes);
						if (signal.aborted) throw new DOMException('Cancelled', 'AbortError');
						ocrStage = 'reading';
						await readPages(
							file,
							password,
							missing,
							ocrReader,
							(page, state) => {
								if (signal.aborted) return;
								const reading = { ...ocrReading };
								if (state.status === 'done') {
									ocrRead.set(`${key}|${page}`, state.words);
									ocrDone++;
									delete reading[page];
								} else reading[page] = state;
								ocrReading = reading;
							},
							signal
						);
					}
					if (signal.aborted) throw new DOMException('Cancelled', 'AbortError');
					ocrStage = 'writing';
					const kept = targets.map((page) =>
						freshWords(ocrRead.get(`${key}|${page}`) ?? [], glyphs[page - 1])
					);
					const empty = targets.filter((_, index) => kept[index].length === 0);
					const words = kept.flat().map(ocrText);
					if (!words.length)
						throw new Error(
							targets.length === 1
								? 'No text was found on this page.'
								: 'No text was found on these pages.'
						);
					const output = await processOcr(file, password, words, signal);
					ocrSkipped = skipped;
					ocrEmpty = empty;
					written = output.bytes;
					return output;
				} finally {
					signal.removeEventListener('abort', stop);
					ocrReading = {};
				}
			},
			'Could not read text from this PDF.',
			() => downloadLink?.click()
		);
		// Find text searches what the engine reads in the result, which is the
		// text layer as any reader will find it.
		if (written && job.result) {
			const result = job.result;
			try {
				const pages = await redactionText(new File([written.slice().buffer], file.name), '');
				if (job.result === result) ocrFound = pages;
			} catch {
				// Searching is a check on the result; the result stands without it.
			}
		}
	}
	// Annotations a box removes whole when it touches them, by page.
	let redactWhole = $state.raw<CropArea[][]>([]);
	// Pages the last result drew from a picture.
	let redactPictured = $state.raw<PicturedPage[]>([]);
	// The page on screen in the single-page preview.
	let previewPage = $state(1);
	let filename = $state('');
	let downloadLink = $state<HTMLAnchorElement>();
	const job = new DownloadJob();
	const processing = $derived(job.processing);
	const error = $derived(job.error);
	const result = $derived(job.result);
	let pdfToImageFormat = $state<'jpg' | 'png'>(
		untrack(() => (tool.id === 'pdf-to-png' ? 'png' : 'jpg'))
	);
	let pdfToImageDpi = $state(150);
	let pdfToImageQuality = $state(80);
	let pdfToImagePageRange = $state('');
	let imagePdfPageSize = $state<'a4' | 'letter' | 'fit'>('a4');
	let imagePdfOrientation = $state<'auto' | 'portrait' | 'landscape'>('auto');
	let imagePdfMargin = $state(18);
	const resultFormat = $derived(job.format);
	const resultSize = $derived(job.size);
	const resultInputSize = $derived(job.inputSize);
	const splitSignature = $derived(
		JSON.stringify({
			mode: splitMode,
			interval: splitInterval,
			combine: splitCombine,
			ranges: splitRanges.map(({ from, to }) => [from, to])
		})
	);
	const organizeSignature = $derived(
		JSON.stringify([organizePages, selectedPages, extractOutput, extractImageFormat, extractDpi])
	);
	const pageToolValid = $derived(
		!!currentFile &&
			organizePages.length > 0 &&
			(isExtract
				? selectedPages.length > 0
				: isRemove
					? selectedPages.length > 0 && selectedPages.length < organizePages.length
					: isRotate
						? organizePages.some((page) => page.rotation !== 0)
						: true)
	);
	const compressSignature = $derived(
		JSON.stringify([compressLevel, compressRemoveMetadata, compressRemoveThumbnails])
	);
	const pdfToImageSignature = $derived(
		JSON.stringify([pdfToImageFormat, pdfToImageDpi, pdfToImageQuality, pdfToImagePageRange])
	);
	const markdownSignature = $derived(JSON.stringify([markdownPages, markdownImages]));
	const printSignature = $derived(
		JSON.stringify([printSize, printLandscape, printMargin, printTypeface])
	);
	const imagePdfSignature = $derived(
		JSON.stringify([
			imagePdfPageSize,
			imagePdfOrientation,
			imagePdfMargin,
			mergeBookmarks,
			protectPassword,
			protectConfirm,
			protectOwner,
			unlockPassword,
			allowPrinting,
			allowCopying,
			allowEditing
		])
	);
	const compressOptions = $derived({
		// clarification here
		// 0 = preserve original, no changes; a higher compression value means better quality otherwise
		// dimension of 0 also means preserve original
		// TODO: should we do image compression with light mode?
		imageQuality: compressLevel === 'light' ? 0 : compressLevel === 'balanced' ? 80 : 50,
		maxImageDimension: compressLevel === 'strong' ? 1600 : compressLevel === 'balanced' ? 2400 : 0,
		removeMetadata: compressRemoveMetadata,
		removeThumbnails: compressRemoveThumbnails
	});
	const splitValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			(splitMode === 'ranges'
				? splitRanges.length > 0 &&
					splitRanges.every(
						({ from, to }) =>
							Number.isInteger(from) &&
							Number.isInteger(to) &&
							from >= 1 &&
							to >= from &&
							to <= pageCount
					)
				: Number.isInteger(splitInterval) && splitInterval > 0)
	);
	const pdfToImageValid = $derived(!!currentFile);
	const protectRestricted = $derived(!(allowPrinting && allowCopying && allowEditing));
	// Only once the confirmation is as long as the password, so typing it does
	// not flash an error at every keystroke.
	const protectMismatch = $derived(
		protectConfirm.length >= protectPassword.length &&
			protectConfirm.length > 0 &&
			protectConfirm !== protectPassword
	);
	const protectOwnerSame = $derived(
		protectRestricted && protectOwner.length > 0 && protectOwner === protectPassword
	);
	// AES-256 passwords are at most 127 bytes of UTF-8.
	const protectTooLong = $derived(
		new TextEncoder().encode(protectPassword).length > 127 ||
			new TextEncoder().encode(protectOwner).length > 127
	);
	const protectValid = $derived(
		!!currentFile &&
			protectPassword.length > 0 &&
			protectConfirm === protectPassword &&
			!protectOwnerSame &&
			!protectTooLong
	);
	const unlockValid = $derived(
		!!currentFile &&
			(unlockProtection === 'restricted' ||
				(unlockProtection === 'password' && unlockPassword.length > 0 && !unlockWrong))
	);
	const imagePdfValid = $derived(workspace.files.length > 0);
	const stampPagesText = $derived(
		isPageNumbers ? numberPages : isCrop ? cropPages : watermarkPages
	);
	const stampPageList = $derived(parsePageRange(stampPagesText, pageCount) ?? []);
	const stampPagesInvalid = $derived(
		stampPagesText.trim() !== '' && pageCount > 0 && stampPageList.length === 0
	);
	// Numbers follow the document's pages from the first one numbered, so a
	// page skipped between two numbered ones still counts.
	const firstNumbered = $derived(stampPageList[0] ?? 1);
	const numberFirstValid = $derived(
		Number.isInteger(numberFirst) && numberFirst >= 0 && numberFirst <= 99999
	);
	const lastNumber = $derived(
		(numberFirstValid ? numberFirst : 1) + (stampPageList.at(-1) ?? pageCount) - firstNumbered
	);
	const watermarkUndrawable = $derived(
		watermarkKind === 'text' ? undrawable(watermarkText) : undefined
	);
	const watermarkMark = $derived<StampMark>(
		watermarkKind === 'text'
			? {
					kind: 'text',
					text: watermarkText,
					family: watermarkFamily,
					bold: watermarkBold,
					size: watermarkSize
				}
			: { kind: 'image', aspect: watermarkImage?.aspect ?? 1, width: watermarkImageWidth / 100 }
	);
	const stampValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			!stampPagesInvalid &&
			(isPageNumbers
				? numberFirstValid
				: watermarkKind === 'text'
					? watermarkText.trim() !== '' && !watermarkUndrawable
					: !!watermarkImage)
	);
	const stampSignature = $derived(
		JSON.stringify([
			numberPosition,
			numberTemplate,
			numberFirst,
			numberPages,
			numberFamily,
			numberBold,
			numberSize,
			numberColor,
			numberMargin,
			watermarkKind,
			watermarkText,
			watermarkFamily,
			watermarkBold,
			watermarkSize,
			watermarkColor,
			watermarkImage?.url,
			watermarkImageWidth,
			watermarkPosition,
			watermarkTile,
			watermarkRotation,
			watermarkOpacity,
			watermarkBehind,
			watermarkMargin,
			watermarkPages
		])
	);
	function cropAreaFor(page: PreviewPage): CropArea {
		if (cropMode === 'manual') return cropArea;
		const bounds = cropBounds[page.number];
		return bounds ? padArea(bounds, cropPadding, page.width, page.height) : FULL_PAGE;
	}
	const cropShownSize = $derived(
		cropPage ? cropSize(cropAreaFor(cropPage), cropPage.width, cropPage.height) : ''
	);
	const cropValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			!stampPagesInvalid &&
			(cropMode === 'auto' || !isFullPage(cropArea))
	);
	const cropSignature = $derived(JSON.stringify([cropMode, cropArea, cropPadding, cropPages]));
	const signPages = $derived(
		signScope === 'one'
			? [signPage]
			: signScope === 'all'
				? Array.from({ length: pageCount }, (_, index) => index + 1)
				: (parsePageRange(signRange, pageCount) ?? [])
	);
	const signRangeInvalid = $derived(
		signScope === 'custom' && signRange.trim() !== '' && pageCount > 0 && signPages.length === 0
	);
	const signValid = $derived(!!currentFile && pageCount > 0 && !!signImage && signPages.length > 0);
	const signSignature = $derived(
		JSON.stringify([signImage?.url, signScope, signRange, signPage, signPlace])
	);
	const flattenFound = $derived(
		typeof flattenScan === 'string'
			? flattenScan
			: flattenScan.flat().filter((mark) => !flattenFormsOnly || mark.field).length
	);
	const flattenValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			flattenFound !== 'checking' &&
			(flattenFound === 'unknown' || flattenFound > 0)
	);
	const redactMatches = $derived(
		isRedact && typeof pageText !== 'string' ? findText(pageText, redactQuery) : []
	);
	const redactUnmarked = $derived(
		redactMatches.filter(
			(match) =>
				!match.boxes.every((box) =>
					covered(
						box,
						redactMarks.filter((mark) => mark.page === match.page).map((mark) => mark.area),
						0.98
					)
				)
		)
	);
	const redactCurrent = $derived(redactMatches[redactMatch]);
	const redactTextState = $derived(
		typeof pageText === 'string'
			? pageText
			: pageText.every((page) => page.ends.length === 0)
				? 'none'
				: 'ready'
	);
	const redactPages = $derived(new Set(redactMarks.map((mark) => mark.page)).size);
	const redactValid = $derived(!!currentFile && pageCount > 0 && redactMarks.length > 0);
	const redactSignature = $derived(
		JSON.stringify([redactMarks, redactFill, redactMetadata, redactAsImages])
	);
	// Pages that had to become pictures, said once, after the result.
	const redactNotice = $derived.by(() => {
		const forced = redactPictured.filter((entry) => entry.reason !== 'chosen');
		if (!forced.length) return '';
		const numbers = forced.map((entry) => entry.page);
		const one = numbers.length === 1;
		const list = one
			? `Page ${numbers[0]}`
			: `Pages ${numbers.slice(0, -1).join(', ')} and ${numbers.at(-1)}`;
		const reasons = new Set(forced.map((entry) => entry.reason));
		const why =
			reasons.size > 1
				? 'parts of them could not be removed in place'
				: {
						text: one
							? 'its text uses a font that cannot be measured'
							: 'their text uses fonts that cannot be measured',
						image: one
							? 'it has an image in a format that cannot be edited'
							: 'they have images in a format that cannot be edited',
						content: one ? 'its content could not be read' : 'their content could not be read',
						chosen: ''
					}[forced[0].reason];
		return `${list} ${one ? 'was redacted as an image' : 'were redacted as images'}, since ${why}.`;
	});
	function redactNote(page: number) {
		const count = redactMarks.filter((mark) => mark.page === page).length;
		return count ? `${count} marked` : undefined;
	}
	function redactEdit() {
		redactHistory = [...redactHistory.slice(-99), redactMarks];
	}
	function addRedactAreas(page: number, areas: CropArea[]) {
		redactEdit();
		const added = areas.map((area) => ({ id: ++redactNextId, page, area }));
		redactMarks = [...redactMarks, ...added];
		redactSelected = added.length === 1 ? added[0].id : null;
	}
	function changeRedactArea(id: number, area: CropArea) {
		redactMarks = redactMarks.map((mark) => (mark.id === id ? { ...mark, area } : mark));
	}
	function removeRedactArea(id: number) {
		redactEdit();
		redactMarks = redactMarks.filter((mark) => mark.id !== id);
		if (redactSelected === id) redactSelected = null;
	}
	function undoRedact() {
		const previous = redactHistory.at(-1);
		if (!previous) return;
		redactHistory = redactHistory.slice(0, -1);
		redactMarks = previous;
		redactSelected = null;
	}
	function clearRedact() {
		redactEdit();
		redactMarks = [];
		redactSelected = null;
	}
	function markAllMatches() {
		redactEdit();
		redactMarks = [
			...redactMarks,
			...redactUnmarked.flatMap((match) =>
				match.boxes.map((area) => ({ id: ++redactNextId, page: match.page, area }))
			)
		];
		redactSelected = null;
	}
	function stepRedactMatch(offset: number) {
		const count = redactMatches.length;
		if (!count) return;
		redactMatch =
			redactMatch < 0 ? (offset > 0 ? 0 : count - 1) : (redactMatch + offset + count) % count;
		previewPage = redactMatches[redactMatch].page;
	}
	const annotateValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			annotate.ready.length > 0 &&
			!annotate.undrawable &&
			!(annotate.flatten && annotate.hasNotes)
	);
	function annotateNote(page: number) {
		const count = annotate.ready.filter((mark) => mark.page === page).length;
		return count ? `${count} added` : undefined;
	}
	const editValid = $derived(
		!!currentFile && pageCount > 0 && edit.ready.length > 0 && !edit.undrawable
	);
	// Said once, after the result, when old content stayed under paint.
	const editNotice = $derived.by(() => {
		if (!editCovered.length) return '';
		const numbers = editCovered.map((entry) => entry.page);
		const one = numbers.length === 1;
		const list = one
			? `page ${numbers[0]}`
			: `pages ${numbers.slice(0, -1).join(', ')} and ${numbers.at(-1)}`;
		const reasons = new Set(editCovered.map((entry) => entry.reason));
		const why =
			reasons.size > 1
				? 'parts of them could not be changed in place'
				: {
						text: one
							? 'its text uses a font that cannot be measured'
							: 'their text uses fonts that cannot be measured',
						image: one
							? 'it has an image in a format that cannot be edited'
							: 'they have images in a format that cannot be edited',
						content: one ? 'its content could not be read' : 'their content could not be read'
					}[editCovered[0].reason];
		return `On ${list}, the old content was painted over rather than removed, since ${why}.`;
	});
	const translateNotice = $derived(
		[
			translateEmpty.length
				? `Nothing was translated on ${pageList(translateEmpty)}, which ${translateEmpty.length === 1 ? 'has' : 'have'} no text; scans need OCR PDF first.`
				: '',
			translateCovered.length
				? `On ${pageList(translateCovered.map((entry) => entry.page))}, the original text was painted over rather than removed.`
				: ''
		]
			.filter(Boolean)
			.join(' ')
	);
	const formChanged = $derived(
		typeof formFields === 'string' ? [] : changedFields(formFields, formValues)
	);
	const formUndrawable = $derived(
		typeof formFields === 'string' ? undefined : undrawableField(formFields, formValues)
	);
	// Whether any field holds something Clear all would take out.
	const formFilled = $derived(
		typeof formFields !== 'string' &&
			Object.entries(blankValues(formFields)).some(
				([name, blank]) => JSON.stringify(formValues[name]) !== JSON.stringify(blank)
			)
	);
	const formsValid = $derived(
		!!currentFile && pageCount > 0 && formChanged.length > 0 && !formUndrawable
	);
	const formSignature = $derived(JSON.stringify([formValues, formFlatten]));
	let formReading = 0;
	async function readForm(pdf: PDFDocumentProxy) {
		const read = ++formReading;
		formFields = 'reading';
		try {
			const fields = await readFormFields(pdf);
			if (read !== formReading) return;
			formFields = fields;
			formValues = { ...fields.initial };
		} catch {
			if (read === formReading) formFields = 'unknown';
		}
	}
	let redactReading = 0;
	async function readPageText(file: File, pdf: PDFDocumentProxy) {
		const read = ++redactReading;
		pageText = 'reading';
		if (isRedact)
			redactRemovals(pdf)
				.then((pages) => {
					if (read === redactReading) redactWhole = pages;
				})
				.catch(() => {});
		try {
			const pages = await redactionText(file, workspace.passwordFor(file));
			if (read === redactReading) pageText = pages;
		} catch {
			if (read === redactReading) pageText = 'unknown';
		}
	}
	$effect(() => {
		void redactQuery;
		redactMatch = -1;
	});
	let flattenScanning = 0;
	async function scanFlatten(pdf: PDFDocumentProxy) {
		const scan = ++flattenScanning;
		flattenScan = 'checking';
		try {
			const marks = await flattenMarks(pdf);
			if (scan === flattenScanning) flattenScan = marks;
		} catch {
			if (scan === flattenScanning) flattenScan = 'unknown';
		}
	}
	function signed(page: number) {
		return signPages.includes(page);
	}
	function stamped(page: number) {
		return stampPageList.length === 0 || stampPageList.includes(page);
	}
	function numberText(page: number) {
		return numberTemplate
			.replace('{total}', String(lastNumber))
			.replace('{n}', String((numberFirstValid ? numberFirst : 1) + page - firstNumbered));
	}
	// Unlock also takes the password in its sidebar, so a locked file there is
	// the expected input rather than something to clear first.
	const locked = $derived(workspace.hasLockedFiles && !isUnlock);
	const actionDisabled = $derived(
		locked
			? true
			: isScan
				? !scanValid || processing || !!dragged || !!keyboardPicked
				: isOcr
					? !ocrValid || processing
					: isTranslate
						? !translateValid || processing
						: isCompare
							? !compareValid || processing
							: isProtect
								? !protectValid || processing
								: isUnlock
									? !unlockValid || processing
									: isPdfA || isRepair
										? !currentFile || processing
										: isFlatten
											? !flattenValid || processing
											: isForms
												? !formsValid || processing
												: isEdit
													? !editValid || processing
													: isAnnotate
														? !annotateValid || processing
														: isRedact
															? !redactValid || processing
															: isSign
																? !signValid || processing
																: isCrop
																	? !cropValid || processing
																	: isStamp
																		? !stampValid || processing
																		: printTool
																			? !printOutput || printBusy || processing
																			: officeTool
																				? !currentFile || processing
																				: isMerge
																					? workspace.files.length < 2 ||
																						processing ||
																						!!dragged ||
																						!!keyboardPicked
																					: isSplit
																						? !splitValid || processing
																						: isPageTool
																							? !pageToolValid || processing
																							: isCompress
																								? workspace.files.length === 0 ||
																									processing ||
																									!!dragged ||
																									!!keyboardPicked
																								: isPdfToImage
																									? !pdfToImageValid || processing
																									: isImageToPdf
																										? !imagePdfValid ||
																											processing ||
																											!!dragged ||
																											!!keyboardPicked
																										: true
	);
	const actionUnavailable = $derived(
		locked
			? true
			: isScan
				? !scanValid || !!dragged || !!keyboardPicked
				: isOcr
					? !ocrValid
					: isTranslate
						? !translateValid
						: isCompare
							? !compareValid
							: isProtect
								? !protectValid
								: isUnlock
									? !unlockValid
									: isPdfA || isRepair
										? !currentFile
										: isFlatten
											? !flattenValid
											: isForms
												? !formsValid
												: isEdit
													? !editValid
													: isAnnotate
														? !annotateValid
														: isRedact
															? !redactValid
															: isSign
																? !signValid
																: isCrop
																	? !cropValid
																	: isStamp
																		? !stampValid
																		: printTool
																			? !printOutput || printBusy
																			: officeTool
																				? !currentFile
																				: isMerge
																					? workspace.files.length < 2 ||
																						!!dragged ||
																						!!keyboardPicked
																					: isSplit
																						? !splitValid
																						: isPageTool
																							? !pageToolValid
																							: isCompress
																								? workspace.files.length === 0 ||
																									!!dragged ||
																									!!keyboardPicked
																								: isPdfToImage
																									? !pdfToImageValid
																									: isImageToPdf
																										? !imagePdfValid ||
																											!!dragged ||
																											!!keyboardPicked
																										: true
	);
	const baseName = $derived(currentFile?.name.replace(/\.[^.]+$/, '') || 'document');
	const autoName = $derived(
		isScan
			? 'plico-scan'
			: isOcr
				? `${baseName}-ocr`
				: isTranslate
					? `${baseName}-${translate.to.toLowerCase()}`
					: isCompare
						? `${compareChanged?.name.replace(/\.[^.]+$/, '') || 'document'}-compared`
						: officeTool || printTool
							? baseName
							: isProtect
								? `${baseName}-protected`
								: isUnlock
									? `${baseName}-unlocked`
									: isPdfA
										? `${baseName}-pdfa`
										: isRepair
											? `${baseName}-repaired`
											: isFlatten
												? `${baseName}-flattened`
												: isForms
													? `${baseName}-filled`
													: isEdit
														? `${baseName}-edited`
														: isAnnotate
															? `${baseName}-annotated`
															: isRedact
																? `${baseName}-redacted`
																: isSign
																	? `${baseName}-signed`
																	: isCrop
																		? `${baseName}-cropped`
																		: isPageNumbers
																			? `${baseName}-numbered`
																			: isWatermark
																				? `${baseName}-watermarked`
																				: isMerge
																					? 'plico-merged'
																					: isImageToPdf
																						? 'plico-images'
																						: isSplit
																							? `${baseName}-split`
																							: isOrganize
																								? `${baseName}-organized`
																								: isExtract
																									? `${baseName}-extracted`
																									: isRemove
																										? `${baseName}-pages-removed`
																										: isRotate
																											? `${baseName}-rotated`
																											: isCompress
																												? `${baseName}-compressed`
																												: `${baseName}-images`
	);
	// Several parts or images arrive as a ZIP; predict which before processing
	// so the filename field shows the extension that will actually download.
	const expectedFormat = $derived(
		officeTool
			? officeTools[officeTool].output
			: isSplit
				? (
						splitMode === 'ranges'
							? splitCombine || splitRanges.length === 1
							: pageCount > 0 && splitInterval >= pageCount
					)
					? 'pdf'
					: 'zip'
				: isExtract && extractOutput === 'images'
					? selectedPages.length === 1
						? extractImageFormat
						: 'zip'
					: isExtract && extractOutput === 'separate' && selectedPages.length > 1
						? 'zip'
						: isPdfToImage
							? (parsePageRange(pdfToImagePageRange, pageCount)?.length ?? pageCount) === 1
								? pdfToImageFormat
								: 'zip'
							: isCompress && workspace.files.length > 1
								? 'zip'
								: 'pdf'
	);
	// Compare shows its result as soon as both PDFs are in; the action saves it.
	const idleLabel = $derived(isCompare ? 'Mark up changes' : tool.label);
	const outputExtension = $derived(result ? resultFormat : expectedFormat);
	const formatLabel = (format: string) => (format === 'md' ? 'Markdown' : format.toUpperCase());
	const downloadName = $derived(
		`${filename.trim().replace(new RegExp(`\\.${outputExtension}$`, 'i'), '') || autoName}.${outputExtension}`
	);
	const continuable = $derived(!!result && resultFormat === 'pdf' && nextTools(tool.id).length > 0);
	const savedPercent = $derived(
		isCompress && result && resultInputSize > 0
			? Math.round((1 - resultSize / resultInputSize) * 100)
			: 0
	);
	let prevInputType = $state(untrack(() => inputType));
	$effect(() => {
		const currentType = inputType;
		if (prevInputType !== currentType) {
			prevInputType = currentType;
			keyboardPicked = null;
			workspace.use(currentType);
		}
	});
	$effect(() => {
		void workspace.files;
		void splitSignature;
		void organizeSignature;
		void compressSignature;
		void pdfToImageSignature;
		void imagePdfSignature;
		void stampSignature;
		void cropSignature;
		void signSignature;
		void flattenFormsOnly;
		void formSignature;
		void redactSignature;
		void annotate.signature;
		void edit.signature;
		void markdownSignature;
		void printSignature;
		void pdfaPart;
		void ocrSignature;
		void translate.signature;
		void scanSignature;
		untrack(() => {
			job.clear();
			ocrFound = null;
			repairFindings = null;
		});
	});
	$effect(() => {
		const files = workspace.files;
		untrack(() => {
			for (const file of [...compareGlyphs.keys()])
				if (!files.includes(file)) compareGlyphs.delete(file);
		});
	});
	$effect(() => {
		void currentFile;
		pageCount = 0;
		splitRanges = [{ id: 0, from: 1, to: 1 }];
		splitMode = 'ranges';
		splitInterval = 1;
		splitCombine = false;
		pdfToImagePageRange = '';
		markdownPages = '';
		numberPages = '';
		watermarkPages = '';
		cropPages = '';
		cropArea = FULL_PAGE;
		cropBounds = {};
		cropPage = null;
		signPage = 1;
		signRange = '';
		signPlace = SIGN_PLACE;
		flattenScanning++;
		flattenScan = 'checking';
		previewPage = 1;
		redactMarks = [];
		redactHistory = [];
		redactSelected = null;
		redactQuery = '';
		redactPictured = [];
		redactReading++;
		pageText = 'reading';
		redactWhole = [];
		annotate.reset();
		edit.reset();
		editCovered = [];
		formReading++;
		formFields = 'reading';
		formValues = {};
		formCanvases = {};
		ocrRead.clear();
		ocrReading = {};
		ocrPages = '';
		translate.reset();
		translateSizes = [];
		translateEmpty = [];
		translateCovered = [];
		ocrFound = null;
		ocrQuery = '';
	});
	// Pages are owned here but reconciled by the viewer as PDFs come and go, so
	// drop selections that no longer name a live page.
	$effect(() => {
		const keys = new Set(organizePages.map(pageKey));
		if (selectedPages.some((key) => !keys.has(key)))
			selectedPages = selectedPages.filter((key) => keys.has(key));
	});
	$effect(() => {
		void tool.id;
		organizePages = [];
		selectedPages = [];
		filename = '';
		protectPassword = '';
		protectConfirm = '';
		protectOwner = '';
		allowPrinting = allowCopying = allowEditing = true;
	});
	// pdf.js cannot tell a restricted PDF from an unprotected one, so Unlock
	// asks the engine what the file actually carries.
	$effect(() => {
		const file = currentFile;
		if (!isUnlock || !file) return;
		let cancelled = false;
		unlockProtection = 'checking';
		unlockPassword = '';
		rejectedPassword = '';
		pdfProtection(file)
			.then((protection) => {
				if (!cancelled) unlockProtection = protection;
			})
			.catch(() => {
				if (!cancelled) unlockProtection = 'none';
			});
		return () => {
			cancelled = true;
		};
	});
	// Repair says up front whether the engine can open the file; a password
	// typed into the card checks it again.
	$effect(() => {
		const file = currentFile;
		if (!isRepair || !file) return;
		const password = workspace.passwordFor(file);
		let cancelled = false;
		repairCondition = 'checking';
		pdfCondition(file, password)
			.then((condition) => {
				if (!cancelled) repairCondition = condition;
			})
			.catch(() => {
				if (!cancelled) repairCondition = 0;
			});
		return () => {
			cancelled = true;
		};
	});
	// Any tool whose run finds a file the engine cannot open repairs the PDFs
	// it was given that are damaged, keeps the copies in their place, and runs
	// again. Files that open are never rewritten.
	job.recover = async (signal) => {
		if (inputType !== 'pdf') return false;
		let repaired = false;
		for (const file of workspace.files) {
			if (workspace.repairedCopy(file)) continue;
			const password = workspace.passwordFor(file);
			if ((await pdfCondition(file, password, signal)) !== 0) continue;
			const outcome = await repairPdf(file, password, signal).catch((cause) => {
				throw cause instanceof DOMException
					? cause
					: new Error(`“${file.name}” is too damaged to open or repair.`);
			});
			workspace.repaired(file, outcome.file);
			repaired = true;
		}
		return repaired;
	};
	// A password accepted in the card fills the sidebar field, so either place
	// works and Unlock is ready to press.
	$effect(() => {
		if (!isUnlock || !currentFile) return;
		const known = workspace.passwordFor(currentFile);
		const lock = workspace.lockState(currentFile);
		if (known && (!lock || lock === 'unlocked')) untrack(() => (unlockPassword = known));
	});
	onDestroy(() => {
		job.clear();
		if (watermarkImage) URL.revokeObjectURL(watermarkImage.url);
		annotate.destroy();
		edit.destroy();
		ocrReader.destroy();
		translate.destroy();
		scan.destroy();
	});
	async function protect() {
		if (processing || !protectValid || !currentFile) return;
		const file = currentFile;
		const options = {
			userPassword: protectPassword,
			ownerPassword: protectRestricted ? protectOwner : '',
			allowPrinting,
			allowCopying,
			allowEditing
		};
		await job.run(
			(signal) => processProtectPdf(file, workspace.passwordFor(file), options, signal),
			'Could not protect this PDF.',
			() => downloadLink?.click()
		);
	}
	async function unlock() {
		if (processing || !unlockValid || !currentFile) return;
		const file = currentFile;
		const password = unlockProtection === 'password' ? unlockPassword : '';
		await job.run(
			async (signal) => ({
				bytes: await unlockPdf(file, password, signal),
				format: 'pdf'
			}),
			'Could not unlock this PDF.',
			() => downloadLink?.click()
		);
		if (job.error.includes('could not be unlocked with that password')) {
			rejectedPassword = password;
			job.error = '';
		} else if (job.result && password) {
			// Lets the card show its pages now that the password is known.
			workspace.unlock(file, password);
		}
	}
	async function convertPdfA() {
		if (processing || !currentFile) return;
		const file = currentFile;
		await job.run(
			async (signal) => ({
				bytes: await convertToPdfA(file, workspace.passwordFor(file), pdfaPart, signal),
				format: 'pdf'
			}),
			'Could not convert this PDF to PDF/A.',
			() => downloadLink?.click()
		);
	}
	async function repair() {
		if (processing || !currentFile) return;
		const file = currentFile;
		await job.run(
			async (signal) => {
				const outcome = await repairPdf(file, workspace.passwordFor(file), signal);
				repairFindings = outcome.findings;
				// Only a file the engine could not open is swapped for its copy;
				// one that opens stays exactly as it was chosen.
				if (outcome.damaged) workspace.repaired(file, outcome.file);
				return { bytes: new Uint8Array(await outcome.file.arrayBuffer()), format: 'pdf' };
			},
			'Could not repair this PDF.',
			() => downloadLink?.click()
		);
	}
	async function addPageNumbers() {
		if (processing || !stampValid || !currentFile) return;
		const file = currentFile;
		const options = {
			template: numberTemplate,
			firstNumber: numberFirst,
			pages: stampPageList,
			position: numberPosition,
			margin: numberMargin,
			family: numberFamily,
			bold: numberBold,
			size: numberSize,
			color: Number.parseInt(numberColor.slice(1), 16),
			opacity: 1
		};
		await job.run(
			(signal) => processPageNumbers(file, workspace.passwordFor(file), options, signal),
			'Could not add page numbers to this PDF.',
			() => downloadLink?.click()
		);
	}
	async function addWatermark() {
		if (processing || !stampValid || !currentFile) return;
		const file = currentFile;
		const image = watermarkKind === 'image' ? watermarkImage?.file : undefined;
		const options = {
			text: watermarkText,
			family: watermarkFamily,
			bold: watermarkBold,
			size: watermarkSize,
			color: Number.parseInt(watermarkColor.slice(1), 16),
			imageWidth: watermarkImageWidth / 100,
			pages: stampPageList,
			position: watermarkPosition,
			margin: watermarkMargin,
			rotation: watermarkRotation,
			opacity: watermarkOpacity / 100,
			behind: watermarkBehind,
			tile: watermarkTile
		};
		await job.run(
			(signal) => processWatermark(file, workspace.passwordFor(file), options, image, signal),
			'Could not add a watermark to this PDF.',
			() => downloadLink?.click()
		);
	}
	async function flatten() {
		if (processing || !flattenValid || !currentFile) return;
		const file = currentFile;
		const formsOnly = flattenFormsOnly;
		await job.run(
			async (signal) => {
				const output = await processFlatten(file, workspace.passwordFor(file), formsOnly, signal);
				flattenKept = output.kept;
				return output;
			},
			'Could not flatten this PDF.',
			() => downloadLink?.click()
		);
	}
	async function fillForm() {
		if (processing || !formsValid || !currentFile || typeof formFields === 'string') return;
		const file = currentFile;
		const options = {
			fills: formFills(formFields, formValues),
			texts: formTexts(formFields, formValues),
			flatten: formFlatten
		};
		await job.run(
			async (signal) => {
				const output = await processFillForm(file, workspace.passwordFor(file), options, signal);
				formKept = output.kept;
				return output;
			},
			'Could not fill this form.',
			() => downloadLink?.click()
		);
	}
	async function redact() {
		if (processing || !redactValid || !currentFile) return;
		const file = currentFile;
		const options = {
			// Plain copies: a reactive proxy cannot be posted to the worker.
			areas: redactMarks.map(({ page, area }) => ({ page, area: [...area] as CropArea })),
			color: redactFill === 'white' ? 0xffffff : 0,
			removeMetadata: redactMetadata,
			asImages: redactAsImages
		};
		redactPictured = [];
		redactSelected = null;
		await job.run(
			async (signal) => {
				const output = await processRedact(file, workspace.passwordFor(file), options, signal);
				redactPictured = output.pictured;
				return output;
			},
			'Could not redact this PDF.',
			() => downloadLink?.click()
		);
	}
	async function addAnnotations() {
		if (processing || !annotateValid || !currentFile) return;
		annotate.finishEditing();
		annotate.select(null);
		const file = currentFile;
		const { annotations, images } = annotate.payload();
		const options = { annotations, remove: [], flatten: annotate.flatten };
		await job.run(
			(signal) => processAnnotate(file, workspace.passwordFor(file), options, images, signal),
			'Could not annotate this PDF.',
			() => downloadLink?.click()
		);
	}
	async function applyEdits() {
		if (processing || !editValid || !currentFile) return;
		edit.finishEditing();
		edit.select(null);
		const file = currentFile;
		const { options, images } = edit.edits();
		editCovered = [];
		await job.run(
			async (signal) => {
				const output = await processEdit(
					file,
					workspace.passwordFor(file),
					options,
					images,
					signal
				);
				editCovered = output.covered;
				return output;
			},
			'Could not edit this PDF.',
			() => downloadLink?.click()
		);
	}
	async function markChanges() {
		if (processing || !compareValid || !comparison || !compareChanged) return;
		const file = compareChanged;
		const options = { annotations: markup(comparison.changes), remove: [], flatten: false };
		await job.run(
			(signal) => processAnnotate(file, workspace.passwordFor(file), options, [], signal),
			'Could not mark up this PDF.',
			() => downloadLink?.click()
		);
	}
	async function sign() {
		if (processing || !signValid || !currentFile || !signImage) return;
		const file = currentFile;
		const image = signImage.file;
		const options = { pages: signPages, place: signPlace };
		await job.run(
			(signal) => processSign(file, workspace.passwordFor(file), image, options, signal),
			'Could not sign this PDF.',
			() => downloadLink?.click()
		);
	}
	async function crop() {
		if (processing || !cropValid || !currentFile) return;
		const file = currentFile;
		const pages = stampPageList.length
			? stampPageList
			: Array.from({ length: pageCount }, (_, index) => index + 1);
		const options: CropOptions =
			cropMode === 'auto'
				? { mode: 'auto', pages, padding: cropPadding }
				: { mode: 'manual', pages, area: cropArea };
		await job.run(
			(signal) => processCrop(file, workspace.passwordFor(file), options, signal),
			'Could not crop this PDF.',
			() => downloadLink?.click()
		);
	}
	// Scans a copy at most 1000 pixels across: plenty to find a margin, and
	// quick enough to run on every page the preview draws.
	function measureCrop(canvas: HTMLCanvasElement, page: PreviewPage) {
		if (!isCrop) return;
		const scale = Math.min(1, 1000 / Math.max(canvas.width, canvas.height));
		const probe = document.createElement('canvas');
		probe.width = Math.max(1, Math.round(canvas.width * scale));
		probe.height = Math.max(1, Math.round(canvas.height * scale));
		const context = probe.getContext('2d', { willReadFrequently: true });
		if (!context) return;
		context.drawImage(canvas, 0, 0, probe.width, probe.height);
		const bounds = contentBounds(context.getImageData(0, 0, probe.width, probe.height));
		cropBounds = { ...cropBounds, [page.number]: bounds ?? null };
		cropPage = page;
	}
	async function merge() {
		if (processing || dragged || workspace.files.length < 2) return;
		const files = [...workspace.files];
		await job.run(
			async (signal) => ({
				bytes: await processPdfs(
					'merge',
					files,
					workspace.passwordsFor(files),
					mergeBookmarks ? files.map((file) => file.name.replace(/\.pdf$/i, '')) : [],
					signal
				),
				format: 'pdf'
			}),
			'Could not merge these PDFs.',
			() => downloadLink?.click()
		);
	}
	async function split() {
		if (processing || !splitValid || !currentFile) return;
		const file = currentFile;
		const options: SplitOptions =
			splitMode === 'ranges'
				? {
						mode: 'ranges',
						ranges: splitRanges.map(({ from, to }) => ({ from, to })),
						combine: splitCombine
					}
				: { mode: 'fixed', interval: splitInterval };
		await job.run(
			(signal) => processSplitPdf(file, workspace.passwordFor(file), options, signal),
			'Could not split this PDF.',
			() => downloadLink?.click()
		);
	}
	async function organize() {
		if (processing || !pageToolValid) return;
		const files = [...workspace.files];
		const selected = new Set(selectedPages);
		if (isExtract && extractOutput === 'separate') {
			const file = files[0];
			const ranges = organizePages
				.filter((page) => selected.has(pageKey(page)))
				.map((page) => ({ from: page.number, to: page.number }));
			await job.run(
				(signal) =>
					processSplitPdf(
						file,
						workspace.passwordFor(file),
						{ mode: 'ranges', ranges, combine: false },
						signal
					),
				'Could not extract pages from this PDF.',
				() => downloadLink?.click()
			);
			return;
		}
		if (isExtract && extractOutput === 'images') {
			const file = files[0];
			const options: PdfImageOptions = {
				format: extractImageFormat,
				dpi: extractDpi,
				quality: 80,
				pages: organizePages
					.filter((page) => selected.has(pageKey(page)))
					.map((page) => page.number)
			};
			await job.run(
				(signal) => processPdfToImages(file, workspace.passwordFor(file), options, signal),
				'Could not extract pages from this PDF.',
				() => downloadLink?.click()
			);
			return;
		}
		const pages = organizePages
			.filter((page) =>
				isExtract ? selected.has(pageKey(page)) : isRemove ? !selected.has(pageKey(page)) : true
			)
			.map((page) => ({ ...page }));
		await job.run(
			(signal) => processOrganizePdf(files, workspace.passwordsFor(files), pages, signal),
			`Could not ${isExtract ? 'extract pages from' : isRemove ? 'remove pages from' : isRotate ? 'rotate pages in' : 'organize'} ${files.length > 1 ? 'these PDFs' : 'this PDF'}.`,
			() => downloadLink?.click()
		);
	}
	async function compress() {
		if (processing || dragged || workspace.files.length === 0) return;
		const files = [...workspace.files];
		const inputSize = files.reduce((total, file) => total + file.size, 0);
		await job.run(
			(signal) => processCompressPdf(files, workspace.passwordsFor(files), compressOptions, signal),
			'Could not compress these PDFs.',
			() => downloadLink?.click(),
			inputSize
		);
	}
	async function convertPdfToImage() {
		if (processing || !pdfToImageValid || !currentFile) return;
		const file = currentFile;
		const options: PdfImageOptions = {
			format: pdfToImageFormat,
			dpi: pdfToImageDpi,
			quality: pdfToImageQuality,
			pages: parsePageRange(pdfToImagePageRange, pageCount)
		};
		await job.run(
			(signal) => processPdfToImages(file, workspace.passwordFor(file), options, signal),
			'Could not convert this PDF to images.',
			() => downloadLink?.click()
		);
	}
	async function convertImagesToPdf() {
		if (processing || !imagePdfValid) return;
		const files = [...workspace.files];
		const options: ImagePdfOptions = {
			pageWidth: imagePdfPageSize === 'fit' ? 0 : imagePdfPageSize === 'letter' ? 612.0 : 595.28,
			pageHeight: imagePdfPageSize === 'fit' ? 0 : imagePdfPageSize === 'letter' ? 792.0 : 841.89,
			margin: imagePdfMargin,
			orientation: imagePdfOrientation
		};
		await job.run(
			(signal) => processImagesToPdf(files, options, signal),
			'Could not convert images to PDF.',
			() => downloadLink?.click()
		);
	}
	async function convertOffice() {
		if (!officeTool || !currentFile || processing) return;
		const operation = officeTool;
		const file = currentFile;
		const markdown = isMarkdown
			? { pages: parsePageRange(markdownPages, pageCount) ?? [], images: markdownImages }
			: undefined;
		officeStage = 'loading';
		markdownText = '';
		await job.run(
			async (signal) => {
				const output = await processOfficeFile(
					operation,
					file,
					workspace.passwordFor(file),
					signal,
					() => (officeStage = 'converting'),
					markdown
				);
				if (output.format === 'md') markdownText = new TextDecoder().decode(output.bytes);
				return output;
			},
			`Could not convert this ${formatName(inputType)} file.`,
			() => downloadLink?.click()
		);
	}
	// The preview is made again whenever the file or an option changes; a
	// change while one is being made cancels it.
	$effect(() => {
		if (!printTool) return;
		const file = currentFile;
		const options = { size: printSize, landscape: printLandscape, margin: printMargin };
		const typeface = printTypeface;
		if (file !== untrack(() => printSource)) {
			printSource = file;
			printPreview = null;
			printOutput = null;
		}
		if (!file) return;
		const controller = new AbortController();
		const delay = setTimeout(
			() => void preparePrint(file, options, typeface, controller.signal),
			untrack(() => printPreview) ? 150 : 0
		);
		return () => {
			clearTimeout(delay);
			controller.abort();
		};
	});
	async function preparePrint(
		file: File,
		options: { size: PrintSize; landscape: boolean; margin: number },
		typeface: Typeface,
		signal: AbortSignal
	) {
		printBusy = true;
		printError = '';
		printOutput = null;
		try {
			const html =
				printTool === 'markdown-to-pdf'
					? markdownDocument(await file.text(), typeface)
					: await readHtml(file);
			const { layout, images, fonts, skipped } = await layoutHtml(html, options, signal);
			if (signal.aborted) return;
			// Not cancelled: a newer preview waits for this one rather than
			// restarting the PDF worker.
			const output = await processPrint(layout, images, fonts);
			if (signal.aborted) return;
			printOutput = output;
			printSkipped = skipped;
			printPreview = new File([output.bytes.slice().buffer], `${baseName}.pdf`, {
				type: 'application/pdf'
			});
		} catch (cause) {
			if (signal.aborted) return;
			printPreview = null;
			printError =
				cause instanceof Error && cause.message
					? cause.message
					: `Could not lay out this ${formatName(inputType)} file.`;
		} finally {
			if (!signal.aborted) printBusy = false;
		}
	}
	async function convertPrint() {
		const output = printOutput;
		if (!output || processing) return;
		await job.run(
			async () => output,
			`Could not convert this ${formatName(inputType)} file.`,
			() => downloadLink?.click()
		);
	}
	function add(files: FileList) {
		workspace.add(files, inputType);
		if (input) input.value = '';
	}
	function replace(files: FileList) {
		const file = Array.from(files).find((item) => acceptsFile(item, inputType));
		if (file) {
			workspace.files = [file];
			workspace.error = '';
		} else if (files.length) {
			workspace.error = `Please choose a ${formatName(inputType)} file.`;
		}
		if (input) input.value = '';
	}
</script>

{#snippet scanCard(file: File)}
	{@const photo = scan.get(file)}
	{@const turn = photo ? pendingTurn(photo, 3 / 4) : { degrees: 0, scale: 1 }}
	<div class="grid aspect-[3/4] place-items-center bg-canvas/50 p-3">
		{#if photo?.preview?.url}
			{#key photo.preview.url}<img
					src={photo.preview.url}
					alt=""
					draggable="false"
					in:fade={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
					out:fade={{ duration: reducedMotion ? 0 : 220, easing: cubicIn }}
					class="col-start-1 row-start-1 max-h-full max-w-full bg-white shadow-md shadow-black/40 motion-safe:transition-transform motion-safe:duration-300 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]"
					style:transform="rotate({turn.degrees}deg) scale({turn.scale})"
				/>{/key}
		{:else if photo?.status === 'failed'}
			<p class="px-2 text-center text-xs text-convert">This photo could not be read</p>
		{:else}
			<div class="col-start-1 row-start-1 h-4/5 w-3/5 animate-pulse rounded-sm bg-white/5"></div>
		{/if}
	</div>
{/snippet}

{#snippet stampOverlay(page: PreviewPage)}
	{#if isTranslate}
		{@const placed = translate.set(page.number)}
		<TranslateOverlay
			{page}
			blocks={translate.blocks[page.number - 1] ?? []}
			texts={placed.texts}
			sets={placed.sets}
			translating={translate.active.includes(page.number)}
			showOriginal={translate.showOriginal}
			rtl={translate.rtl}
			editable={!processing}
			{reducedMotion}
			onedit={(block, text) => translate.edit(block, text)}
			oncover={(block, color) => translate.cover(block, color)}
		/>
	{:else if isOcr}
		<OcrOverlay
			state={ocrState(page.number)}
			matches={ocrMatches
				.filter((match) => match.page === page.number && match !== ocrCurrent)
				.flatMap((match) => match.boxes)}
			current={ocrCurrent?.page === page.number ? ocrCurrent.boxes : []}
			{reducedMotion}
		/>
	{:else if isForms}
		{#if typeof formFields !== 'string'}
			<FormOverlay
				{page}
				widgets={formFields.pages[page.number - 1]?.widgets ?? []}
				drawn={formFields.pages[page.number - 1]?.drawn ?? []}
				canvases={formCanvases[page.number]}
				values={formValues}
				invalid={formUndrawable?.name}
				editable={!processing}
				onchange={(name, value) => (formValues = { ...formValues, [name]: value })}
			/>
		{/if}
	{:else if isAnnotate || isEdit}
		<AnnotateOverlay
			{page}
			editor={isEdit ? edit : annotate}
			glyphs={typeof pageText === 'string' ? undefined : pageText[page.number - 1]}
			editable={!processing}
			{reducedMotion}
		/>
	{:else if isRedact}
		<RedactOverlay
			{page}
			marks={redactMarks.filter((mark) => mark.page === page.number)}
			selected={redactSelected}
			glyphs={typeof pageText === 'string' ? undefined : pageText[page.number - 1]}
			matches={redactUnmarked
				.filter((match) => match.page === page.number && match !== redactCurrent)
				.flatMap((match) => match.boxes)}
			current={redactCurrent?.page === page.number ? redactCurrent.boxes : []}
			whole={redactWhole[page.number - 1] ?? []}
			fill={redactFill === 'white' ? '#ffffff' : '#000000'}
			editable={!processing}
			{reducedMotion}
			onadd={(areas) => addRedactAreas(page.number, areas)}
			onedit={redactEdit}
			onchange={changeRedactArea}
			onselect={(id) => (redactSelected = id)}
			onremove={removeRedactArea}
		/>
	{:else if isFlatten}
		<FlattenOverlay
			marks={typeof flattenScan === 'string' ? [] : (flattenScan[page.number - 1] ?? [])}
			formsOnly={flattenFormsOnly}
		/>
	{:else if isSign}
		<SignOverlay
			{page}
			place={signPlace}
			imageUrl={signImage?.url ?? ''}
			aspect={signImage?.aspect ?? 1}
			placed={signed(page.number)}
			editable={!processing && (signed(page.number) || signScope === 'one')}
			{reducedMotion}
			onchange={(place) => {
				signPlace = place;
				if (signScope === 'one') signPage = page.number;
			}}
		/>
	{:else if isCrop}
		<CropOverlay
			area={cropAreaFor(page)}
			width={page.width}
			height={page.height}
			editable={cropMode === 'manual' && !processing}
			drawable={!processing}
			{reducedMotion}
			onchange={(area) => {
				// Drawing on the page in Content mode means choosing the area by hand.
				cropArea = area;
				cropMode = 'manual';
			}}
		/>
	{:else if isPageNumbers}
		<StampOverlay
			width={page.width}
			height={page.height}
			mark={{
				kind: 'text',
				text: numberText(page.number),
				family: numberFamily,
				bold: numberBold,
				size: numberSize
			}}
			placement={{ position: numberPosition, margin: numberMargin, rotation: 0, tile: false }}
			color={numberColor}
			opacity={1}
			{reducedMotion}
		/>
	{:else}
		<StampOverlay
			width={page.width}
			height={page.height}
			mark={watermarkMark}
			placement={{
				position: watermarkPosition,
				margin: watermarkMargin,
				rotation: watermarkRotation,
				tile: watermarkTile
			}}
			color={watermarkColor}
			opacity={watermarkOpacity / 100}
			imageUrl={watermarkKind === 'image' ? watermarkImage?.url : ''}
			{reducedMotion}
			onlayout={(layout) => (watermarkTooDense = layout.tooDense)}
		/>
	{/if}
{/snippet}

<main id="main-content" class="flex flex-1 flex-col">
	{#if !supported}
		<div class="grid flex-1 place-items-center px-6 py-16">
			<div
				class="flex w-full max-w-md flex-col items-center gap-4 rounded-2xl bg-panel px-8 py-14 text-center"
			>
				<tool.icon size={36} stroke={1.5} class={accent} />
				<h1 class="text-xl font-semibold tracking-tight">{tool.label}</h1>
				<p class="text-sm leading-relaxed text-muted">
					This tool is listed in the toolbox but is not available yet. It has not been built — try
					Merge, Split, Compress, Organize, or a conversion in the meantime.
				</p>
			</div>
		</div>
	{:else}
		<div
			class="grid flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[minmax(0,1fr)_22rem] xl:grid-cols-[minmax(0,1fr)_24rem] {continuable
				? 'max-lg:pb-64'
				: workspace.files.length
					? 'max-lg:pb-24'
					: ''}"
		>
			<section
				aria-label="Documents"
				data-drag-area
				class="relative isolate flex min-w-0 flex-col overflow-clip {isCompare
					? 'px-6 pt-6 pb-2 lg:pb-6'
					: 'px-6 pt-6 sm:px-10 lg:px-16 lg:pt-10'} {isCompare
					? ''
					: isSplit || isPdfToImage || isPageTool || isPagePreview || printTool
						? 'pb-2 lg:pb-16'
						: 'pb-12 lg:pb-20'}"
				ondragover={(event) => event.preventDefault()}
				ondrop={(event) => {
					if (
						!event.defaultPrevented &&
						!dragged &&
						!keyboardPicked &&
						event.dataTransfer?.files.length
					) {
						event.preventDefault();
						if (!processing) {
							if (isCompare) placeCompare(null, event.dataTransfer.files);
							else if (
								isSplit ||
								isPdfToImage ||
								isProtect ||
								isUnlock ||
								isPdfA ||
								isRepair ||
								isPagePreview ||
								printTool ||
								officeTool ||
								(isPageTool && !isOrganize)
							)
								replace(event.dataTransfer.files);
							else workspace.add(event.dataTransfer.files, inputType);
						}
					}
				}}
			>
				<img
					src="/hero-contours.svg"
					alt=""
					aria-hidden="true"
					class="pointer-events-none absolute -right-48 -bottom-48 -z-10 w-240 max-w-none opacity-[0.07] select-none"
				/>
				<div class="grid min-h-0 flex-1">
					{#if workspace.files.length === 0}
						<div
							in:fade={{ duration: reducedMotion ? 0 : 260, easing: cubicOut }}
							out:fade={{ duration: reducedMotion ? 0 : 150, easing: cubicIn }}
							class="col-start-1 row-start-1 mx-auto flex w-full max-w-xl flex-col justify-center py-12"
						>
							<div class="h-80"><PdfDropzone selectedTool={tool} emptyOnly /></div>
							{#if isScan && cameraAvailable}<button
									type="button"
									class="mx-auto mt-5 flex items-center gap-2 rounded-xl border-2 border-white/10 bg-panel px-4 py-3 text-sm font-semibold text-muted transition-colors hover:border-convert/40 hover:text-convert"
									onclick={() => (scanCamera = true)}><IconCamera size={18} />Use camera</button
								>{/if}
						</div>
					{:else}
						<div
							in:fade={{ duration: reducedMotion ? 0 : 260, easing: cubicOut }}
							out:fade={{ duration: reducedMotion ? 0 : 150, easing: cubicIn }}
							class="col-start-1 row-start-1 flex min-w-0 flex-col"
						>
							{#if isOrganize || (!isCompare && !isSplit && !isPageTool && !isPdfToImage && !isProtect && !isUnlock && !isPdfA && !isRepair && !isPagePreview && !officeTool && !printTool)}<input
									bind:this={input}
									type="file"
									accept={inputAccept[inputType]}
									multiple
									class="hidden"
									aria-label={isImageToPdf || isScan ? 'Add images' : 'Add PDF files'}
									onchange={() => input?.files && add(input.files)}
								/>{/if}
							{#if isCompare}
								<div class="w-full py-6 lg:py-0">
									<CompareViewer
										original={compareOriginal}
										changed={compareChanged}
										{comparison}
										bind:current={compareCurrent}
										mode={compareMode}
										highlight={compareHighlight}
										{reducedMotion}
										onload={(_, file) => void readCompareText(file)}
										onremove={(side) => {
											const file = side === 'original' ? compareOriginal : compareChanged;
											if (file) workspace.remove(file);
										}}
										onswap={() => {
											if (compareOriginal && compareChanged)
												workspace.files = [compareChanged, compareOriginal];
										}}
										onplace={placeCompare}
									/>
								</div>
							{:else if isSplit && currentFile}
								<div class="my-auto w-full py-6 lg:py-10">
									{#key currentFile}<SplitViewer
											file={currentFile}
											ranges={splitRanges}
											mode={splitMode}
											interval={splitInterval}
											onrangechange={(id, from, to) =>
												(splitRanges = splitRanges.map((range) =>
													range.id === id ? { ...range, from, to } : range
												))}
											onload={(count) => {
												pageCount = count;
												if (
													splitRanges.length === 1 &&
													splitRanges[0].from === 1 &&
													splitRanges[0].to === 1
												)
													splitRanges[0].to = count;
											}}
											onremove={() => workspace.remove(currentFile)}
										/>{/key}
								</div>
							{:else if (isAnnotate || isForms || isEdit || isOcr || isTranslate) && currentFile}
								<div class="w-full py-6 lg:pb-0">
									{#key currentFile}<PageScroller
											bind:this={scroller}
											file={currentFile}
											bind:current={previewPage}
											overlay={stampOverlay}
											forms={isForms
												? (page, canvases) => (formCanvases = { ...formCanvases, [page]: canvases })
												: undefined}
											onload={(count, pdf) => {
												pageCount = count;
												if (isForms) void readForm(pdf);
												else void readPageText(currentFile, pdf);
												if (isTranslate) {
													translatePdfProxy = pdf;
													void readPageSizes(pdf);
												}
											}}
											onremove={() => workspace.remove(currentFile)}
										/>{/key}
								</div>
							{:else if printTool && currentFile}
								<div class="w-full py-6 lg:pb-0">
									<PrintPreview
										file={currentFile}
										preview={printPreview}
										error={printError}
										onload={(count) => (pageCount = count)}
										onremove={() => workspace.remove(currentFile)}
									/>
								</div>
							{:else if isPagePreview && currentFile}
								<div class="my-auto w-full py-6 lg:py-10">
									{#key currentFile}<StampPreview
											file={currentFile}
											behind={isWatermark && watermarkBehind}
											stamped={isSign ? signed : isAnnotate ? () => true : stamped}
											overlayEverywhere={isSign || isRedact || isAnnotate}
											large={isRedact || isCrop || isSign || isAnnotate}
											bind:current={previewPage}
											note={isRedact ? redactNote : isAnnotate ? annotateNote : undefined}
											{reducedMotion}
											overlay={stampOverlay}
											onload={(count, pdf) => {
												pageCount = count;
												if (isFlatten) void scanFlatten(pdf);
												if (isRedact || isAnnotate) void readPageText(currentFile, pdf);
											}}
											onrender={measureCrop}
											onremove={() => workspace.remove(currentFile)}
										/>{/key}
								</div>
							{:else if isPageTool && currentFile}
								<div class="flex min-h-0 w-full flex-1 flex-col py-6 lg:py-0">
									{#key isOrganize ? 'organize' : currentFile}<OrganizeViewer
											bind:this={organizeViewer}
											files={workspace.files}
											pages={organizePages}
											mode={isExtract
												? 'extract'
												: isRemove
													? 'remove'
													: isRotate
														? 'rotate'
														: 'organize'}
											selected={selectedPages}
											{processing}
											{reducedMotion}
											onpageschange={(pages) => (organizePages = pages)}
											onselectionchange={(keys) => (selectedPages = keys)}
											onadd={() => input?.click()}
											onremove={(file) => workspace.remove(file)}
										/>{/key}
								</div>
							{:else if isScan && scanEditing && scan.get(scanEditing)}
								{@const editing = scanEditing}
								<div
									class="my-auto w-full py-6 lg:py-10"
									in:fade={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
								>
									{#key editing}<div
											in:fade={{ duration: reducedMotion ? 0 : 200, easing: cubicOut }}
										>
											<ScanEditor
												file={editing}
												photo={scan.get(editing)!}
												index={workspace.files.indexOf(editing)}
												count={workspace.files.length}
												{reducedMotion}
												disabled={processing}
												onchange={(corners) => scan.setCorners(editing, corners)}
												onturn={(quarter) => scan.turn(editing, quarter)}
												onfind={() => {
													const found = scan.get(editing)?.found;
													if (found) scan.setCorners(editing, found);
												}}
												onwhole={() => scan.setCorners(editing, FULL_PHOTO)}
												onstep={stepScanEditor}
												onclose={() => (scanEditing = null)}
											/>
										</div>{/key}
								</div>
							{:else}
								<DocumentCards
									mode={cardMode}
									{accent}
									officeFormat={inputType === 'docx' || inputType === 'pptx' || inputType === 'xlsx'
										? inputType
										: undefined}
									{processing}
									{reducedMotion}
									bind:dragged
									bind:keyboardPicked
									onadd={() => input?.click()}
									onload={(count) => {
										if (!pageCount) pageCount = count;
									}}
									preview={isScan ? scanCard : undefined}
									onopen={isScan ? (file) => (scanEditing = file) : undefined}
									onturn={isScan ? (file) => scan.turn(file, 1) : undefined}
									oncamera={isScan && cameraAvailable ? () => (scanCamera = true) : undefined}
								/>
							{/if}
							{#if workspace.error}<p role="alert" class="mt-4 text-sm text-convert">
									{workspace.error}
								</p>{/if}
						</div>
					{/if}
				</div>
			</section>
			<aside
				aria-label={`${tool.label} settings`}
				class="m-6 flex flex-col rounded-2xl bg-panel lg:sticky lg:top-6 lg:ml-0 lg:h-[calc(100svh-9rem)] lg:self-start lg:overflow-y-auto"
			>
				<div class="flex-1 space-y-7 p-6 sm:p-8">
					<h1 class="flex items-center gap-3 text-xl font-semibold tracking-tight">
						<tool.icon size={24} stroke={1.7} class={`shrink-0 ${accent}`} />{tool.label}
					</h1>
					{#if isProtect}
						<div class="space-y-4">
							<PasswordInput
								label="Password"
								bind:value={protectPassword}
								invalid={protectTooLong}
								disabled={processing}
							/>
							<div>
								<PasswordInput
									label="Confirm password"
									bind:value={protectConfirm}
									invalid={protectMismatch}
									disabled={processing}
									onenter={() => void protect()}
								/>
								{#if protectMismatch}<p
										role="alert"
										transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
										class="pt-2 text-xs text-convert"
									>
										Passwords don't match
									</p>{/if}
							</div>
						</div>
					{:else if isPdfA}
						<PdfASettings bind:part={pdfaPart} {accent} {processing} />
					{:else if isRepair && currentFile}
						<div class="flex items-center justify-between">
							<h2 class="text-sm font-semibold">Condition</h2>
							<span
								class="text-xs motion-safe:transition-colors {repairFindings?.length
									? accent
									: repairFindings === null && repairCondition === 0
										? 'text-convert'
										: 'text-muted'}"
								role="status"
								>{repairFindings
									? repairFindings.length
										? 'Repaired'
										: 'No damage found'
									: repairCondition === 'checking'
										? 'Checking...'
										: repairCondition === 0
											? 'Damaged'
											: repairCondition === -1
												? 'Locked'
												: 'Opens normally'}</span
							>
						</div>
						{#if repairFindings?.length}<ul
								transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
								class="space-y-3"
								aria-label="What was repaired"
							>
								{#each repairFindings as finding (finding)}<li
										class="flex items-start gap-2.5 text-sm"
									>
										<IconCheck size={16} stroke={2.2} class="mt-0.5 shrink-0 {accent}" />{finding}
									</li>{/each}
							</ul>{/if}
					{:else if isUnlock && currentFile}
						<div class="flex items-center justify-between">
							<h2 class="text-sm font-semibold">Protection</h2>
							<span class="text-xs text-muted" role="status"
								>{unlockProtection === 'checking'
									? 'Checking...'
									: unlockProtection === 'password'
										? 'Password'
										: unlockProtection === 'restricted'
											? 'Restrictions only'
											: 'None'}</span
							>
						</div>
						{#if unlockProtection === 'password'}<div
								transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							>
								<PasswordInput
									label="Password"
									bind:value={unlockPassword}
									invalid={unlockWrong}
									disabled={processing}
									onenter={() => void unlock()}
								/>
								{#if unlockWrong}<p
										role="alert"
										transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
										class="pt-2 text-xs text-convert"
									>
										Incorrect password
									</p>{/if}
							</div>{/if}
					{:else if isScan}
						<ScanSettings
							look={scan.look}
							onlook={(look) => scan.setLook(look)}
							bind:paper={scanPaper}
							bind:searchable={scanSearchable}
							bind:languages={scanLanguages}
							models={ocrModels}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isTranslate}
						<TranslateSettings
							bind:from={translate.from}
							bind:to={translate.to}
							bind:pages={translate.pages}
							bind:keepOriginal={translate.keepOriginal}
							bind:showOriginal={translate.showOriginal}
							translated={translate.translated}
							detected={translate.detected}
							models={translate.models}
							{pageCount}
							pagesInvalid={translate.pagesInvalid}
							noText={!!translate.glyphs && !translate.hasText}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isOcr}
						<OcrSettings
							bind:languages={ocrLanguages}
							bind:pages={ocrPages}
							bind:skipText={ocrSkipText}
							bind:query={ocrQuery}
							models={ocrModels}
							{pageCount}
							pagesInvalid={ocrPagesInvalid}
							searchable={!!ocrFound}
							matches={ocrMatches.length}
							match={ocrMatch}
							onstep={stepOcrMatch}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isCompare}
						<CompareSettings
							bind:mode={compareMode}
							bind:highlight={compareHighlight}
							bind:current={compareCurrent}
							text={compareTextState}
							changes={comparison?.changes ?? []}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isForms}
						<FormSettings
							changed={formChanged.length}
							filled={formFilled}
							undrawable={formUndrawable}
							onreset={() => {
								if (typeof formFields !== 'string') formValues = { ...formFields.initial };
							}}
							onclear={() => {
								if (typeof formFields !== 'string')
									formValues = { ...formValues, ...blankValues(formFields) };
							}}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isAnnotate || isEdit}
						<AnnotateSettings
							editor={isEdit ? edit : annotate}
							page={previewPage}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isRedact}
						<RedactSettings
							bind:query={redactQuery}
							bind:fill={redactFill}
							text={redactTextState}
							matches={redactMatches.length}
							match={redactMatch}
							unmarked={redactUnmarked.length}
							onstep={stepRedactMatch}
							onmarkall={markAllMatches}
							areas={redactMarks.length}
							pages={redactPages}
							canUndo={redactHistory.length > 0}
							onundo={undoRedact}
							onclear={clearRedact}
							disabled={processing}
						/>
					{:else if isFlatten}
						<FlattenSettings bind:formsOnly={flattenFormsOnly} disabled={processing} />
					{:else if isSign}
						<SignSettings
							bind:image={signImage}
							bind:pages={signScope}
							bind:range={signRange}
							page={signPage}
							{pageCount}
							rangeInvalid={signRangeInvalid}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isCrop}
						<CropSettings
							bind:mode={cropMode}
							bind:padding={cropPadding}
							bind:pages={cropPages}
							size={cropShownSize}
							changed={!isFullPage(cropArea)}
							canFit={!!cropPage && !!cropBounds[cropPage.number]}
							onfit={() => {
								const bounds = cropPage && cropBounds[cropPage.number];
								if (cropPage && bounds)
									cropArea = padArea(bounds, cropPadding, cropPage.width, cropPage.height);
							}}
							onreset={() => (cropArea = FULL_PAGE)}
							{pageCount}
							pagesInvalid={stampPagesInvalid}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isPageNumbers}
						<PageNumberSettings
							bind:position={numberPosition}
							bind:template={numberTemplate}
							bind:firstNumber={numberFirst}
							bind:pages={numberPages}
							{lastNumber}
							{pageCount}
							pagesInvalid={stampPagesInvalid}
							family={numberFamily}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isWatermark}
						<WatermarkSettings
							bind:kind={watermarkKind}
							bind:text={watermarkText}
							bind:family={watermarkFamily}
							bind:bold={watermarkBold}
							bind:size={watermarkSize}
							bind:color={watermarkColor}
							bind:image={watermarkImage}
							bind:imageWidth={watermarkImageWidth}
							bind:position={watermarkPosition}
							bind:tile={watermarkTile}
							bind:rotation={watermarkRotation}
							bind:opacity={watermarkOpacity}
							undrawable={watermarkUndrawable}
							tooDense={watermarkTooDense && watermarkTile}
							{reducedMotion}
							disabled={processing}
						/>
					{:else if isOrganize}
						<div>
							<h2 class="mb-3 text-sm font-semibold">Pages</h2>
							<div
								class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
								role="group"
								aria-label="Pages"
							>
								<button
									type="button"
									onclick={() => (organizePages = [...organizePages].reverse())}
									disabled={processing || organizePages.length < 2}
									class="flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
									><IconArrowsSort size={16} />Reverse</button
								>
								<button
									type="button"
									onclick={() => void organizeViewer?.addBlankPage()}
									disabled={processing || organizePages.length === 0}
									class="flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
									><IconFilePlus size={16} />Blank page</button
								>
							</div>
						</div>
					{:else if isSplit}
						{#key currentFile}<SplitSettings
								{pageCount}
								{reducedMotion}
								bind:ranges={splitRanges}
								bind:mode={splitMode}
								bind:interval={splitInterval}
								bind:combine={splitCombine}
							/>{/key}
					{:else if isExtract || isRemove}
						<PageSelectSettings
							pages={organizePages}
							bind:selected={selectedPages}
							mode={isExtract ? 'extract' : 'remove'}
							{processing}
							{reducedMotion}
							bind:output={extractOutput}
							bind:imageFormat={extractImageFormat}
							bind:dpi={extractDpi}
						/>
					{:else if isCompress}
						<CompressSettings bind:level={compressLevel} />
					{:else if isPdfToImage}
						<PdfToImageSettings bind:format={pdfToImageFormat} bind:dpi={pdfToImageDpi} />
					{:else if printTool}
						<PrintSettings
							bind:size={printSize}
							bind:landscape={printLandscape}
							bind:margin={printMargin}
							bind:typeface={printTypeface}
							markdown={printTool === 'markdown-to-pdf'}
						/>
					{:else if isImageToPdf}
						<ImageToPdfSettings
							{reducedMotion}
							bind:pageSize={imagePdfPageSize}
							bind:orientation={imagePdfOrientation}
							bind:margin={imagePdfMargin}
						/>
					{/if}
					{#if workspace.files.length}<div
							in:rangeReveal={{ reducedMotion, preview: true }}
							out:rangeCollapse={{ reducedMotion }}
						>
							<OutputFilename
								bind:value={filename}
								placeholder={autoName}
								extension={outputExtension}
							/>
						</div>{/if}
					<!-- Advanced options always close the settings, below the filename. -->
					{#if isMerge}
						<AdvancedOptions {reducedMotion}>
							<ToggleSwitch
								bind:checked={mergeBookmarks}
								label="Add a bookmark for each file"
								tone="merge"
							/>
						</AdvancedOptions>
					{:else if isProtect}
						<AdvancedOptions {reducedMotion}>
							<ToggleSwitch bind:checked={allowPrinting} label="Allow printing" tone="merge" />
							<ToggleSwitch bind:checked={allowCopying} label="Allow copying text" tone="merge" />
							<ToggleSwitch bind:checked={allowEditing} label="Allow editing" tone="merge" />
							{#if protectRestricted}<div
									transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
									class="pt-3"
								>
									<PasswordInput
										label="Permissions password"
										placeholder="Optional"
										bind:value={protectOwner}
										invalid={protectOwnerSame}
										disabled={processing}
									/>
									{#if protectOwnerSame}<p role="alert" class="pt-2 text-xs text-convert">
											Must differ from the password
										</p>{/if}
								</div>{/if}
						</AdvancedOptions>
					{:else if isPageNumbers}
						<AdvancedOptions {reducedMotion} spacing="space-y-6">
							<StampTextSettings
								bind:family={numberFamily}
								bind:bold={numberBold}
								bind:size={numberSize}
								bind:color={numberColor}
								minSize={6}
								maxSize={36}
								disabled={processing}
							/>
							<StampAdvanced bind:margin={numberMargin} disabled={processing} />
						</AdvancedOptions>
					{:else if isWatermark}
						<AdvancedOptions {reducedMotion} spacing="space-y-6">
							<StampAdvanced
								bind:margin={watermarkMargin}
								marginLabel={watermarkTile ? 'Spacing' : 'Margin'}
								bind:behind={watermarkBehind}
								bind:pages={watermarkPages}
								{pageCount}
								pagesInvalid={stampPagesInvalid}
								disabled={processing}
							/>
						</AdvancedOptions>
					{:else if isForms}
						<AdvancedOptions {reducedMotion}>
							<ToggleSwitch bind:checked={formFlatten} label="Flatten form" tone="brand" />
						</AdvancedOptions>
					{:else if isAnnotate}
						<AdvancedOptions {reducedMotion}>
							<ToggleSwitch
								bind:checked={annotate.flatten}
								label="Draw into the page"
								tone="brand"
							/>
							{#if annotate.flatten && annotate.hasNotes}<p
									role="alert"
									transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
									class="pt-1 text-xs text-convert"
								>
									Notes can't be drawn into the page
								</p>{/if}
						</AdvancedOptions>
					{:else if isRedact}
						<AdvancedOptions {reducedMotion}>
							<ToggleSwitch bind:checked={redactMetadata} label="Remove metadata" tone="merge" />
							<ToggleSwitch
								bind:checked={redactAsImages}
								label="Turn redacted pages into images"
								tone="merge"
							/>
						</AdvancedOptions>
					{:else if isCompress}
						<AdvancedOptions {reducedMotion}>
							<ToggleSwitch
								bind:checked={compressRemoveMetadata}
								label="Remove metadata"
								tone="compress"
							/>
							<ToggleSwitch
								bind:checked={compressRemoveThumbnails}
								label="Remove page thumbnails"
								tone="compress"
							/>
						</AdvancedOptions>
					{:else if isMarkdown}
						<AdvancedOptions {reducedMotion} spacing="space-y-4">
							<PageRangeField bind:value={markdownPages} {pageCount} />
							<ToggleSwitch bind:checked={markdownImages} label="Embed images" tone="split" />
						</AdvancedOptions>
					{:else if isPdfToImage}
						<AdvancedOptions {reducedMotion} spacing="space-y-4">
							<PdfToImageAdvanced
								format={pdfToImageFormat}
								bind:quality={pdfToImageQuality}
								bind:pageRange={pdfToImagePageRange}
								{pageCount}
							/>
						</AdvancedOptions>
					{/if}
				</div>
				<!-- On phones the action stays pinned to the bottom once there is
				something to process, instead of sitting below every page card. -->
				<div
					class="shrink-0 space-y-4 p-6 sm:p-8 {workspace.files.length
						? 'max-lg:fixed max-lg:inset-x-0 max-lg:bottom-0 max-lg:z-20 max-lg:border-t max-lg:border-white/10 max-lg:bg-canvas/85 max-lg:px-6 max-lg:py-4 max-lg:backdrop-blur-md'
						: ''}"
					aria-live="polite"
				>
					<a
						bind:this={downloadLink}
						href={result}
						rel="external"
						download={downloadName}
						class="hidden"
						tabindex="-1"
						aria-hidden="true">Download {formatLabel(resultFormat)}</a
					>
					<div class="flex">
						<button
							disabled={actionDisabled}
							onclick={() =>
								result
									? downloadLink?.click()
									: isScan
										? void scanToPdf()
										: isOcr
											? void readText()
											: isTranslate
												? void translatePdf()
												: isCompare
													? void markChanges()
													: isProtect
														? void protect()
														: isUnlock
															? void unlock()
															: isPdfA
																? void convertPdfA()
																: isRepair
																	? void repair()
																	: isFlatten
																		? void flatten()
																		: isForms
																			? void fillForm()
																			: isEdit
																				? void applyEdits()
																				: isAnnotate
																					? void addAnnotations()
																					: isRedact
																						? void redact()
																						: isSign
																							? void sign()
																							: isCrop
																								? void crop()
																								: isPageNumbers
																									? void addPageNumbers()
																									: isWatermark
																										? void addWatermark()
																										: isSplit
																											? void split()
																											: isPageTool
																												? void organize()
																												: printTool
																													? void convertPrint()
																													: officeTool
																														? void convertOffice()
																														: isCompress
																															? void compress()
																															: isPdfToImage
																																? void convertPdfToImage()
																																: isImageToPdf
																																	? void convertImagesToPdf()
																																	: void merge()}
							aria-label={result
								? `Download ${formatLabel(resultFormat)} again${savedPercent > 0 ? `, ${savedPercent}% smaller` : ''}`
								: processing
									? job.repairing || isRepair
										? 'Repairing PDF'
										: isScan
											? scanStatus
											: isOcr
												? ocrStatus
												: isTranslate
													? translateStatus
													: isCompare
														? 'Marking up changes'
														: isProtect
															? 'Protecting PDF'
															: isUnlock
																? 'Unlocking PDF'
																: isPdfA
																	? 'Converting to PDF/A'
																	: isFlatten
																		? 'Flattening PDF'
																		: isForms
																			? 'Filling form'
																			: isEdit
																				? 'Editing PDF'
																				: isAnnotate
																					? 'Annotating PDF'
																					: isRedact
																						? 'Redacting PDF'
																						: isSign
																							? 'Signing PDF'
																							: isCrop
																								? 'Cropping PDF'
																								: isPageNumbers
																									? 'Adding page numbers'
																									: isWatermark
																										? 'Adding watermark'
																										: isSplit
																											? 'Splitting PDF'
																											: isPageTool
																												? `${tool.label} in progress`
																												: printTool
																													? 'Converting to PDF...'
																													: officeTool
																														? officeStage === 'loading'
																															? 'Loading converter...'
																															: `Converting to ${formatLabel(officeTools[officeTool].output)}...`
																														: isCompress
																															? 'Compressing PDF'
																															: isPdfToImage
																																? `Converting to ${pdfToImageFormat.toUpperCase()}...`
																																: isImageToPdf
																																	? 'Converting images to PDF...'
																																	: 'Merging PDF'
									: idleLabel}
							class="group relative isolate flex min-h-14 w-full items-center justify-center overflow-hidden rounded-xl px-4 py-4 text-sm font-bold text-canvas transition-[background-color,filter,transform] duration-200 enabled:hover:brightness-110 disabled:cursor-not-allowed motion-safe:enabled:active:scale-[0.985] {buttonColor} {actionUnavailable
								? 'opacity-40'
								: ''}"
						>
							<span
								class="pointer-events-none absolute inset-0 origin-left bg-white/15 motion-safe:transition-transform motion-safe:duration-500 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {result
									? 'scale-x-100'
									: 'scale-x-0'}"
								aria-hidden="true"
							></span>
							<span class="relative z-10 grid place-items-center">
								<span
									aria-hidden={!!result}
									class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap motion-safe:transition-[opacity,transform] motion-safe:duration-200 {result
										? '-translate-y-2 opacity-0'
										: 'translate-y-0 opacity-100'}"
									>{#if processing}<IconLoader2 class="animate-spin" size={20} />{job.repairing ||
										isRepair
											? 'Repairing...'
											: isScan
												? scanStatus
												: isOcr
													? ocrStatus
													: isTranslate
														? translateStatus
														: printTool
															? 'Converting...'
															: officeTool
																? officeStage === 'loading'
																	? 'Loading converter...'
																	: 'Converting...'
																: isCompare
																	? 'Marking up...'
																	: isProtect
																		? 'Protecting...'
																		: isUnlock
																			? 'Unlocking...'
																			: isPdfA
																				? 'Converting...'
																				: isFlatten
																					? 'Flattening...'
																					: isForms
																						? 'Filling...'
																						: isEdit
																							? 'Editing...'
																							: isAnnotate
																								? 'Annotating...'
																								: isRedact
																									? 'Redacting...'
																									: isSign
																										? 'Signing...'
																										: isCrop
																											? 'Cropping...'
																											: isPageNumbers
																												? 'Numbering...'
																												: isWatermark
																													? 'Watermarking...'
																													: isSplit
																														? 'Splitting...'
																														: isPageTool
																															? 'Processing...'
																															: isCompress
																																? 'Compressing...'
																																: isPdfToImage
																																	? 'Converting...'
																																	: isImageToPdf
																																		? 'Converting...'
																																		: 'Merging...'}{:else}
										{idleLabel}<IconArrowRight size={20} />{/if}</span
								>
								<span
									aria-hidden={!result}
									class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap motion-safe:transition-[opacity,transform] motion-safe:duration-300 {result
										? 'translate-y-0 opacity-100 motion-safe:delay-100'
										: 'translate-y-2 opacity-0'}"
									><IconDownload size={20} />Download {formatLabel(
										resultFormat
									)}{#if savedPercent > 0}<span
											class="rounded-md bg-canvas/15 px-1.5 py-0.5 text-xs font-semibold"
											>−{savedPercent}%</span
										>{/if}</span
								>
							</span></button
						>
						{#if isMarkdown && result && markdownText}<div
								transition:slide={{
									axis: 'x',
									duration: reducedMotion ? 0 : 260,
									easing: cubicOut
								}}
								class="shrink-0 pl-3"
							>
								<CopyButton text={markdownText} label="Copy Markdown" />
							</div>{/if}
					</div>
					{#if error}<p role="alert" class="text-sm text-convert">{error}</p>{/if}
					{#if isOcr && result && ocrNotice}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{ocrNotice}
						</p>{/if}
					{#if isTranslate && result && translateNotice}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{translateNotice}
						</p>{/if}
					{#if printTool && printPreview && printSkipped > 0}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{printSkipped} linked {printSkipped === 1 ? 'image was' : 'images were'} left out. Only
							images saved inside the file are included.
						</p>{/if}
					{#if isEdit && result && editNotice}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{editNotice}
						</p>{/if}
					{#if isRedact && result && redactNotice}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{redactNotice}
						</p>{/if}
					{#if isForms && result && formFlatten && formKept > 0}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{formKept}
							{formKept === 1 ? 'field has' : 'fields have'} nothing stored to draw and stayed fillable
						</p>{/if}
					{#if isFlatten && result && flattenKept > 0}<p
							role="status"
							transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
							class="text-xs leading-relaxed text-muted"
						>
							{flattenKept}
							{flattenKept === 1 ? 'item has' : 'items have'} no stored appearance to draw and stayed
							as
							{flattenKept === 1 ? 'it was' : 'they were'}
						</p>{/if}
					{#if continuable}<div
							in:rangeReveal={{ reducedMotion, preview: true }}
							out:rangeCollapse={{ reducedMotion }}
						>
							<ContinueWith
								{tool}
								result={() =>
									job.blob && new File([job.blob], downloadName, { type: 'application/pdf' })}
							/>
						</div>{/if}
				</div>
			</aside>
		</div>
	{/if}
	{#if scanCamera}<ScanCamera
			{reducedMotion}
			oncapture={(file) => workspace.add([file], 'image')}
			onclose={() => (scanCamera = false)}
		/>{/if}
</main>
