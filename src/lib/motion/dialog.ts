import gsap from 'gsap';
import { spring } from './link';

const reduced = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

// The panel is clipped rather than scaled, so its text stays sharp while it
// grows out of the control that opened it and shrinks back into it. Both
// directions start from wherever the panel is, so closing mid-open turns back.
// The dialog needs the `grows` class from app.css for its backdrop and content fade.
export class DialogMotion {
	closing = false;
	private dialog: HTMLDialogElement | undefined;
	private trigger: HTMLElement | undefined;
	private timeline: gsap.core.Timeline | undefined;
	private overflow: string | undefined;

	attach = (dialog: HTMLDialogElement) => {
		this.dialog = dialog;
		const events = new AbortController();
		const options = { signal: events.signal };
		dialog.addEventListener(
			'cancel',
			(event) => {
				event.preventDefault();
				this.close();
			},
			options
		);
		dialog.addEventListener(
			'click',
			(event) => {
				if (event.target !== dialog) return;
				const { left, right, top, bottom } = dialog.getBoundingClientRect();
				if (
					event.clientX < left ||
					event.clientX > right ||
					event.clientY < top ||
					event.clientY > bottom
				)
					this.close();
			},
			options
		);
		dialog.addEventListener('close', () => this.restore(), options);
		return () => {
			events.abort();
			if (dialog.open) dialog.close();
			this.restore();
			this.dialog = undefined;
		};
	};

	open(trigger?: HTMLElement) {
		const dialog = this.dialog;
		if (!dialog || dialog.open) return;
		this.trigger = trigger;
		this.closing = false;
		this.overflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		dialog.showModal();
		dialog.scrollTop = 0;
		this.timeline?.kill();
		if (reduced()) return;
		this.timeline = gsap
			.timeline()
			.fromTo(
				dialog,
				{ ...this.origin(dialog), '--backdrop-opacity': 0, filter: 'blur(0px)' },
				{
					x: 0,
					y: 0,
					clipPath: 'inset(0px 0px round 24px)',
					'--backdrop-opacity': 0.5,
					duration: 0.42,
					ease: spring,
					clearProps: 'transform,clipPath,--backdrop-opacity'
				}
			)
			.to(dialog, { filter: 'blur(1px)', duration: 0.05, ease: 'sine.out' }, 0)
			.to(
				dialog,
				{ filter: 'blur(0px)', duration: 0.37, ease: 'power2.out', clearProps: 'filter' },
				0.05
			);
	}

	/// `target` closes into another control than the one that opened it, as
	/// choosing a tool closes into the navbar's Tools button.
	close(target?: HTMLElement) {
		const dialog = this.dialog;
		if (!dialog?.open || this.closing) return;
		this.closing = true;
		if (target) this.trigger = target;
		this.timeline?.kill();
		if (reduced()) {
			dialog.close();
			return;
		}
		this.timeline = gsap
			.timeline({ onComplete: () => dialog.close() })
			.to(dialog, { ...this.origin(dialog), '--backdrop-opacity': 0, duration: 0.38, ease: spring })
			.to(dialog, { filter: 'blur(0.8px)', duration: 0.04, ease: 'sine.out' }, 0)
			.to(dialog, { filter: 'blur(0px)', duration: 0.34, ease: 'power2.out' }, 0.04)
			.to(dialog, { '--content-opacity': 0, duration: 0.1, ease: 'sine.inOut' }, 0.02)
			.to(dialog, { opacity: 0, duration: 0.14, ease: 'sine.inOut' }, 0.16);
	}

	private origin(dialog: HTMLDialogElement) {
		const panel = dialog.getBoundingClientRect();
		const source = this.trigger?.isConnected ? this.trigger.getBoundingClientRect() : undefined;
		const width = Math.min(source?.width ?? 80, panel.width);
		const height = Math.min(source?.height ?? 44, panel.height);
		const left = panel.left - Number(gsap.getProperty(dialog, 'x'));
		const top = panel.top - Number(gsap.getProperty(dialog, 'y'));
		return {
			x: source ? source.left + source.width / 2 - left - panel.width / 2 : 0,
			y: source ? source.top + source.height / 2 - top - panel.height / 2 : 0,
			clipPath: `inset(${(panel.height - height) / 2}px ${(panel.width - width) / 2}px round 12px)`
		};
	}

	private restore() {
		this.timeline?.kill();
		this.timeline = undefined;
		// clearProps also drops GSAP's cached transform, which origin() reads.
		if (this.dialog)
			gsap.set(this.dialog, {
				clearProps: 'transform,opacity,clipPath,filter,--backdrop-opacity,--content-opacity'
			});
		if (this.overflow !== undefined) document.body.style.overflow = this.overflow;
		this.overflow = undefined;
		this.closing = false;
		this.trigger?.focus({ preventScroll: true });
		this.trigger = undefined;
	}
}
