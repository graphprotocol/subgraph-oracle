import * as wasm from './pkg';
import * as copy from 'copy-to-clipboard';
import notie from 'notie';
import { editor as monacoEditor } from 'monaco-editor/esm/vs/editor/editor.api'
import { output } from './webpack.config';

require('notie/dist/notie.min.css');

const samplePayload = `{
	"version": "v0.0.1",
	"config": {
		"ipfs_concurrency": "4",
		"ipfs_timeout": "10000",
		"min_signal": "100",
		"period": "300",
		"grace_period": "0",
		"supported_data_source_kinds": "ethereum,ethereum/contract,file/ipfs,substreams,file/arweave",
		"network_subgraph_deloyment_id": "QmSWxvd8SaQK6qZKJ7xtfxCCGoRzGnoi2WNzmJYYJW9BXY",
		"epoch_block_oracle_subgraph_deloyment_id": "QmQEGDTb3xeykCXLdWx7pPX3qeeGMUvHmGWP4SpMkv5QJf",
		"subgraph_availability_manager_contract": "CONTRACT_ADDRESS",
		"oracle_index": "ORACLE_INDEX"
	}
}
`;

// https://github.com/microsoft/monaco-editor/issues/2874
self.MonacoEnvironment = {
	getWorkerUrl: function (moduleId, label) {
		return './json.worker.bundle.js';
	}
};

var editor = monacoEditor.create(document.getElementById('container'), {
	value: samplePayload,
	language: 'json',
	minimap: {
		enabled: false
	},
	theme: 'vs-light'
});

document.getElementById('compile-button').onclick = function () {
	let input = editor.getValue();

	try {
		let outputType = (<HTMLSelectElement>document.getElementById('output-type')).value;
		let isCalldata = outputType === 'calldata';
		let compiled = wasm.compile(input, isCalldata);
		(<HTMLInputElement>document.getElementById('compiled')).value = toHexString(compiled);
	}
	catch (e: any) {
		notie.alert({ text: (<string>e), time: 2, type: 'error' });
	}
};

document.getElementById('copy-to-clipboard').onclick = function () {
	let compiled = (<HTMLInputElement>document.getElementById('compiled')).value;
	notie.alert({ text: `Copied ${compiled.length} characters to the clipboard.`, time: 1, type: 'success' });
	copy(compiled);
};

document.getElementById('clear-all').onclick = function () {
	editor.setValue('');
	(<HTMLFormElement>document.getElementById("form")).reset();
}

document.getElementById('verify-compiled').oninput = function () {
	let compiled = (<HTMLInputElement>document.getElementById('compiled')).value;
	let expected = (<HTMLInputElement>document.getElementById('verify-compiled')).value;

	let text;
	if (compiled === expected) {
		text = '✓ matches'
	} else {
		text = '✗ does not match'
	}

	(<HTMLParagraphElement>document.getElementById('verify-result')).innerText = text;
}


function toHexString(byteArray: Uint8Array): string {
	var s = '';
	byteArray.forEach(function (byte) {
		s += ('0' + (byte & 0xFF).toString(16)).slice(-2);
	});
	return s;
}