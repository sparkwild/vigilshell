import Ajv from 'ajv';
import schema from './generated/runtime.schema.json';
import { semanticRule } from './semantic-validation';

// Never coerce, fill defaults or remove fields at a communication boundary.
const ajv = new Ajv({ allErrors: true, strict: false, validateFormats: false });
ajv.addKeyword({ keyword: 'x-vigil-rule', schemaType: 'string', validate: semanticRule });
ajv.addSchema(schema, 'contract');
export function contractValidator<T>(name: keyof typeof schema.definitions) {
  return ajv.compile<T>({ $ref: `contract#/definitions/${name}` });
}
