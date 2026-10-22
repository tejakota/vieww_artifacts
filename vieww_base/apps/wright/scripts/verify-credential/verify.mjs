// Independent check of a Wright receipt credential (receipt.vc.json) with
// Digital Bazaar's reference Data Integrity stack (eddsa-jcs-2022). Nothing
// from Wright runs here. Usage: npm install && node verify.mjs <receipt.vc.json>
// It checks the proof only; WHO the issuer is (its did:key) is for you to
// compare with the key you were given — `wright trust me` prints it.
import fs from 'node:fs';
import jsigs from 'jsonld-signatures';
import {DataIntegrityProof} from '@digitalbazaar/data-integrity';
import {createVerifyCryptosuite} from '@digitalbazaar/eddsa-jcs-2022-cryptosuite';
const cryptosuite = createVerifyCryptosuite();
import * as Ed25519Multikey from '@digitalbazaar/ed25519-multikey';
import {contexts as credContexts} from '@digitalbazaar/credentials-context';

const doc = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const vm = doc.proof.verificationMethod;
const [did, frag] = vm.split('#');
const key = await Ed25519Multikey.from({
  '@context': 'https://w3id.org/security/multikey/v1',
  type: 'Multikey', id: vm, controller: did, publicKeyMultibase: frag,
});
const keyDoc = await key.export({publicKey: true, includeContext: true});
const contexts = new Map(credContexts);
const loader = async url => {
  if (contexts.has(url)) return {contextUrl: null, documentUrl: url, document: contexts.get(url)};
  if (url === vm) return {contextUrl: null, documentUrl: url, document: keyDoc};
  if (url === did) return {contextUrl: null, documentUrl: url, document: {
    '@context': ['https://www.w3.org/ns/did/v1', 'https://w3id.org/security/multikey/v1'],
    id: did, verificationMethod: [keyDoc], assertionMethod: [vm]}};
  try {
    const m = await import('@digitalbazaar/security-context');
    if (m.contexts?.has(url)) return {contextUrl: null, documentUrl: url, document: m.contexts.get(url)};
  } catch {}
  throw new Error('no offline copy of ' + url);
};
const result = await jsigs.verify(doc, {
  suite: new DataIntegrityProof({cryptosuite}),
  purpose: new jsigs.purposes.AssertionProofPurpose({controller: {
    '@context': ['https://www.w3.org/ns/did/v1', 'https://w3id.org/security/multikey/v1'],
    id: did, assertionMethod: [vm], verificationMethod: [keyDoc]}}),
  documentLoader: loader,
});
console.log(result.verified ? 'VERIFIED by Digital Bazaar eddsa-jcs-2022' : 'NOT VERIFIED: ' + JSON.stringify(result.error?.errors?.map(e => e.message) ?? result.error?.message));
process.exit(result.verified ? 0 : 1);
