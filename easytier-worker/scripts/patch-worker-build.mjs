import fs from 'node:fs';
import path from 'node:path';

const buildIndexPath = path.resolve('build/index.js');
const source = fs.readFileSync(buildIndexPath, 'utf8');
const broken = 's=new WebAssembly.Instance(nt,Z()).exports,s.__wbindgen_start()}';
const fixed = 's=new WebAssembly.Instance(nt,Z()).exports,typeof s.__wbindgen_start==="function"&&s.__wbindgen_start()}';

if (!source.includes(broken)) {
  if (source.includes(fixed)) {
    console.log('worker-build patch already applied');
    process.exit(0);
  }

  console.error('worker-build patch target not found');
  process.exit(1);
}

fs.writeFileSync(buildIndexPath, source.replace(broken, fixed));
console.log('patched build/index.js to guard optional __wbindgen_start');
