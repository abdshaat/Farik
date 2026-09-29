// jsdom 30.1.1 has no HTMLDialogElement.showModal or close, so define them
// only when missing. They set and remove `open`; Playwright tests the real ones.
const proto = HTMLDialogElement.prototype;

if (!proto.showModal) {
	proto.showModal = function showModal(this: HTMLDialogElement) {
		this.setAttribute("open", "");
	};
}

if (!proto.close) {
	proto.close = function close(this: HTMLDialogElement) {
		this.removeAttribute("open");
		this.dispatchEvent(new Event("close"));
	};
}
