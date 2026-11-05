/* Generated from Rust DTOs. Run npm run contracts:generate; do not edit. */
import { semanticRule } from '../semantic-validation';
"use strict";
export const validateConnectionPolicy = validate323;
const schema115 = {"type":"object","properties":{"connectionId":{"type":"string","maxLength":128,"minLength":1,"pattern":"^[A-Za-z0-9_-]+$"},"connectionRevision":{"type":"integer","format":"uint64","maximum":9007199254740991,"minimum":0},"execution":{"$ref":"#/definitions/ExecutionPolicy"},"revision":{"type":"integer","format":"uint64","maximum":9007199254740991,"minimum":0},"verifyConnect":{"type":"boolean"}},"additionalProperties":false,"required":["connectionId","connectionRevision","revision","verifyConnect","execution"]};
const func7 = (function ucs2length(str) {
    const len = str.length;
    let length = 0;
    let pos = 0;
    let value;
    while (pos < len) {
        length++;
        value = str.charCodeAt(pos++);
        if (value >= 0xd800 && value <= 0xdbff && pos < len) {
            // high surrogate, and there is a next character
            value = str.charCodeAt(pos);
            if ((value & 0xfc00) === 0xdc00)
                pos++; // low surrogate
        }
    }
    return length;
});
const pattern0 = new RegExp("^[A-Za-z0-9_-]+$", "u");
const schema41 = {"type":"object","properties":{"downloadRoots":{"type":"array","items":{"$ref":"#/definitions/LocalDirectoryGrant"}},"fileTransfer":{"type":"boolean"},"level":{"$ref":"#/definitions/OperationLevel"},"limits":{"$ref":"#/definitions/ExecutionLimits"},"protectSystem":{"type":"boolean"},"protectedPaths":{"type":"array","items":{"$ref":"#/definitions/ProtectedPath"}},"scriptReview":{"type":"boolean","default":false},"uploadRoots":{"type":"array","items":{"$ref":"#/definitions/LocalDirectoryGrant"}},"version":{"type":"integer","format":"uint32","const":2,"minimum":0}},"additionalProperties":false,"required":["version","level","fileTransfer","protectSystem","protectedPaths","uploadRoots","downloadRoots","limits"],"x-vigil-rule":"executionPolicy"};
const schema42 = {"type":"object","properties":{"id":{"type":"string"},"path":{"type":"string"}},"additionalProperties":false,"required":["id","path"]};
const schema43 = {"type":"string","enum":["disabled","readOnly","operate"]};
const schema44 = {"type":"object","properties":{"concurrentTasks":{"type":"integer","format":"uint16","maximum":65535,"minimum":0},"concurrentTerminals":{"type":"integer","format":"uint16","maximum":65535,"minimum":0},"concurrentTransfers":{"type":"integer","format":"uint16","maximum":65535,"minimum":0},"maxTaskSeconds":{"type":["integer","null"],"format":"uint32","minimum":0},"terminalIdleSeconds":{"type":["integer","null"],"format":"uint32","minimum":0},"terminalSeconds":{"type":["integer","null"],"format":"uint32","minimum":0},"transferSeconds":{"type":["integer","null"],"format":"uint32","minimum":0},"transferStallSeconds":{"type":["integer","null"],"format":"uint32","minimum":0}},"additionalProperties":false,"required":["concurrentTasks","concurrentTransfers","concurrentTerminals"]};
const func2 = Object.prototype.hasOwnProperty;
const schema45 = {"type":"object","properties":{"kind":{"$ref":"#/definitions/ScopeKind"},"path":{"type":"string"}},"additionalProperties":false,"required":["path","kind"]};
const schema46 = {"type":"string","enum":["directory","file"]};

function validate35(data, {instancePath="", parentData, parentDataProperty, rootData=data}={}){
let vErrors = null;
let errors = 0;
if(data && typeof data == "object" && !Array.isArray(data)){
if(data.path === undefined){
const err0 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "path"},message:"must have required property '"+"path"+"'"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
if(data.kind === undefined){
const err1 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "kind"},message:"must have required property '"+"kind"+"'"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
for(const key0 in data){
if(!((key0 === "kind") || (key0 === "path"))){
const err2 = {instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
}
if(data.kind !== undefined){
let data0 = data.kind;
if(typeof data0 !== "string"){
const err3 = {instancePath:instancePath+"/kind",schemaPath:"#/definitions/ScopeKind/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
if(!((data0 === "directory") || (data0 === "file"))){
const err4 = {instancePath:instancePath+"/kind",schemaPath:"#/definitions/ScopeKind/enum",keyword:"enum",params:{allowedValues: schema46.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
}
if(data.path !== undefined){
if(typeof data.path !== "string"){
const err5 = {instancePath:instancePath+"/path",schemaPath:"#/properties/path/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
}
}
else {
const err6 = {instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
validate35.errors = vErrors;
return errors === 0;
}


function validate34(data, {instancePath="", parentData, parentDataProperty, rootData=data}={}){
let vErrors = null;
let errors = 0;
if(!semanticRule("executionPolicy", data)){
const err0 = {instancePath,schemaPath:"#/x-vigil-rule",keyword:"x-vigil-rule",params:{},message:"must pass \"x-vigil-rule\" keyword validation"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
if(data && typeof data == "object" && !Array.isArray(data)){
if(data.version === undefined){
const err1 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "version"},message:"must have required property '"+"version"+"'"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
if(data.level === undefined){
const err2 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "level"},message:"must have required property '"+"level"+"'"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
if(data.fileTransfer === undefined){
const err3 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "fileTransfer"},message:"must have required property '"+"fileTransfer"+"'"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
if(data.protectSystem === undefined){
const err4 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "protectSystem"},message:"must have required property '"+"protectSystem"+"'"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
if(data.protectedPaths === undefined){
const err5 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "protectedPaths"},message:"must have required property '"+"protectedPaths"+"'"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
if(data.uploadRoots === undefined){
const err6 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "uploadRoots"},message:"must have required property '"+"uploadRoots"+"'"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
if(data.downloadRoots === undefined){
const err7 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "downloadRoots"},message:"must have required property '"+"downloadRoots"+"'"};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
if(data.limits === undefined){
const err8 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "limits"},message:"must have required property '"+"limits"+"'"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
for(const key0 in data){
if(!(func2.call(schema41.properties, key0))){
const err9 = {instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
}
if(data.downloadRoots !== undefined){
let data0 = data.downloadRoots;
if(Array.isArray(data0)){
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
let data1 = data0[i0];
if(data1 && typeof data1 == "object" && !Array.isArray(data1)){
if(data1.id === undefined){
const err10 = {instancePath:instancePath+"/downloadRoots/" + i0,schemaPath:"#/definitions/LocalDirectoryGrant/required",keyword:"required",params:{missingProperty: "id"},message:"must have required property '"+"id"+"'"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
if(data1.path === undefined){
const err11 = {instancePath:instancePath+"/downloadRoots/" + i0,schemaPath:"#/definitions/LocalDirectoryGrant/required",keyword:"required",params:{missingProperty: "path"},message:"must have required property '"+"path"+"'"};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
for(const key1 in data1){
if(!((key1 === "id") || (key1 === "path"))){
const err12 = {instancePath:instancePath+"/downloadRoots/" + i0,schemaPath:"#/definitions/LocalDirectoryGrant/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
if(data1.id !== undefined){
if(typeof data1.id !== "string"){
const err13 = {instancePath:instancePath+"/downloadRoots/" + i0+"/id",schemaPath:"#/definitions/LocalDirectoryGrant/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
}
if(data1.path !== undefined){
if(typeof data1.path !== "string"){
const err14 = {instancePath:instancePath+"/downloadRoots/" + i0+"/path",schemaPath:"#/definitions/LocalDirectoryGrant/properties/path/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
}
}
}
else {
const err15 = {instancePath:instancePath+"/downloadRoots/" + i0,schemaPath:"#/definitions/LocalDirectoryGrant/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err15];
}
else {
vErrors.push(err15);
}
errors++;
}
}
}
else {
const err16 = {instancePath:instancePath+"/downloadRoots",schemaPath:"#/properties/downloadRoots/type",keyword:"type",params:{type: "array"},message:"must be array"};
if(vErrors === null){
vErrors = [err16];
}
else {
vErrors.push(err16);
}
errors++;
}
}
if(data.fileTransfer !== undefined){
if(typeof data.fileTransfer !== "boolean"){
const err17 = {instancePath:instancePath+"/fileTransfer",schemaPath:"#/properties/fileTransfer/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err17];
}
else {
vErrors.push(err17);
}
errors++;
}
}
if(data.level !== undefined){
let data5 = data.level;
if(typeof data5 !== "string"){
const err18 = {instancePath:instancePath+"/level",schemaPath:"#/definitions/OperationLevel/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err18];
}
else {
vErrors.push(err18);
}
errors++;
}
if(!(((data5 === "disabled") || (data5 === "readOnly")) || (data5 === "operate"))){
const err19 = {instancePath:instancePath+"/level",schemaPath:"#/definitions/OperationLevel/enum",keyword:"enum",params:{allowedValues: schema43.enum},message:"must be equal to one of the allowed values"};
if(vErrors === null){
vErrors = [err19];
}
else {
vErrors.push(err19);
}
errors++;
}
}
if(data.limits !== undefined){
let data6 = data.limits;
if(data6 && typeof data6 == "object" && !Array.isArray(data6)){
if(data6.concurrentTasks === undefined){
const err20 = {instancePath:instancePath+"/limits",schemaPath:"#/definitions/ExecutionLimits/required",keyword:"required",params:{missingProperty: "concurrentTasks"},message:"must have required property '"+"concurrentTasks"+"'"};
if(vErrors === null){
vErrors = [err20];
}
else {
vErrors.push(err20);
}
errors++;
}
if(data6.concurrentTransfers === undefined){
const err21 = {instancePath:instancePath+"/limits",schemaPath:"#/definitions/ExecutionLimits/required",keyword:"required",params:{missingProperty: "concurrentTransfers"},message:"must have required property '"+"concurrentTransfers"+"'"};
if(vErrors === null){
vErrors = [err21];
}
else {
vErrors.push(err21);
}
errors++;
}
if(data6.concurrentTerminals === undefined){
const err22 = {instancePath:instancePath+"/limits",schemaPath:"#/definitions/ExecutionLimits/required",keyword:"required",params:{missingProperty: "concurrentTerminals"},message:"must have required property '"+"concurrentTerminals"+"'"};
if(vErrors === null){
vErrors = [err22];
}
else {
vErrors.push(err22);
}
errors++;
}
for(const key2 in data6){
if(!((((((((key2 === "concurrentTasks") || (key2 === "concurrentTerminals")) || (key2 === "concurrentTransfers")) || (key2 === "maxTaskSeconds")) || (key2 === "terminalIdleSeconds")) || (key2 === "terminalSeconds")) || (key2 === "transferSeconds")) || (key2 === "transferStallSeconds"))){
const err23 = {instancePath:instancePath+"/limits",schemaPath:"#/definitions/ExecutionLimits/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key2},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err23];
}
else {
vErrors.push(err23);
}
errors++;
}
}
if(data6.concurrentTasks !== undefined){
let data7 = data6.concurrentTasks;
if(!((typeof data7 == "number") && (!(data7 % 1) && !isNaN(data7)))){
const err24 = {instancePath:instancePath+"/limits/concurrentTasks",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTasks/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err24];
}
else {
vErrors.push(err24);
}
errors++;
}
if(typeof data7 == "number"){
if(data7 > 65535 || isNaN(data7)){
const err25 = {instancePath:instancePath+"/limits/concurrentTasks",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTasks/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err25];
}
else {
vErrors.push(err25);
}
errors++;
}
if(data7 < 0 || isNaN(data7)){
const err26 = {instancePath:instancePath+"/limits/concurrentTasks",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTasks/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err26];
}
else {
vErrors.push(err26);
}
errors++;
}
}
}
if(data6.concurrentTerminals !== undefined){
let data8 = data6.concurrentTerminals;
if(!((typeof data8 == "number") && (!(data8 % 1) && !isNaN(data8)))){
const err27 = {instancePath:instancePath+"/limits/concurrentTerminals",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTerminals/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err27];
}
else {
vErrors.push(err27);
}
errors++;
}
if(typeof data8 == "number"){
if(data8 > 65535 || isNaN(data8)){
const err28 = {instancePath:instancePath+"/limits/concurrentTerminals",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTerminals/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err28];
}
else {
vErrors.push(err28);
}
errors++;
}
if(data8 < 0 || isNaN(data8)){
const err29 = {instancePath:instancePath+"/limits/concurrentTerminals",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTerminals/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err29];
}
else {
vErrors.push(err29);
}
errors++;
}
}
}
if(data6.concurrentTransfers !== undefined){
let data9 = data6.concurrentTransfers;
if(!((typeof data9 == "number") && (!(data9 % 1) && !isNaN(data9)))){
const err30 = {instancePath:instancePath+"/limits/concurrentTransfers",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTransfers/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err30];
}
else {
vErrors.push(err30);
}
errors++;
}
if(typeof data9 == "number"){
if(data9 > 65535 || isNaN(data9)){
const err31 = {instancePath:instancePath+"/limits/concurrentTransfers",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTransfers/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"};
if(vErrors === null){
vErrors = [err31];
}
else {
vErrors.push(err31);
}
errors++;
}
if(data9 < 0 || isNaN(data9)){
const err32 = {instancePath:instancePath+"/limits/concurrentTransfers",schemaPath:"#/definitions/ExecutionLimits/properties/concurrentTransfers/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err32];
}
else {
vErrors.push(err32);
}
errors++;
}
}
}
if(data6.maxTaskSeconds !== undefined){
let data10 = data6.maxTaskSeconds;
if((!((typeof data10 == "number") && (!(data10 % 1) && !isNaN(data10)))) && (data10 !== null)){
const err33 = {instancePath:instancePath+"/limits/maxTaskSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/maxTaskSeconds/type",keyword:"type",params:{type: schema44.properties.maxTaskSeconds.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err33];
}
else {
vErrors.push(err33);
}
errors++;
}
if(typeof data10 == "number"){
if(data10 < 0 || isNaN(data10)){
const err34 = {instancePath:instancePath+"/limits/maxTaskSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/maxTaskSeconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err34];
}
else {
vErrors.push(err34);
}
errors++;
}
}
}
if(data6.terminalIdleSeconds !== undefined){
let data11 = data6.terminalIdleSeconds;
if((!((typeof data11 == "number") && (!(data11 % 1) && !isNaN(data11)))) && (data11 !== null)){
const err35 = {instancePath:instancePath+"/limits/terminalIdleSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/terminalIdleSeconds/type",keyword:"type",params:{type: schema44.properties.terminalIdleSeconds.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err35];
}
else {
vErrors.push(err35);
}
errors++;
}
if(typeof data11 == "number"){
if(data11 < 0 || isNaN(data11)){
const err36 = {instancePath:instancePath+"/limits/terminalIdleSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/terminalIdleSeconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err36];
}
else {
vErrors.push(err36);
}
errors++;
}
}
}
if(data6.terminalSeconds !== undefined){
let data12 = data6.terminalSeconds;
if((!((typeof data12 == "number") && (!(data12 % 1) && !isNaN(data12)))) && (data12 !== null)){
const err37 = {instancePath:instancePath+"/limits/terminalSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/terminalSeconds/type",keyword:"type",params:{type: schema44.properties.terminalSeconds.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err37];
}
else {
vErrors.push(err37);
}
errors++;
}
if(typeof data12 == "number"){
if(data12 < 0 || isNaN(data12)){
const err38 = {instancePath:instancePath+"/limits/terminalSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/terminalSeconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err38];
}
else {
vErrors.push(err38);
}
errors++;
}
}
}
if(data6.transferSeconds !== undefined){
let data13 = data6.transferSeconds;
if((!((typeof data13 == "number") && (!(data13 % 1) && !isNaN(data13)))) && (data13 !== null)){
const err39 = {instancePath:instancePath+"/limits/transferSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/transferSeconds/type",keyword:"type",params:{type: schema44.properties.transferSeconds.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err39];
}
else {
vErrors.push(err39);
}
errors++;
}
if(typeof data13 == "number"){
if(data13 < 0 || isNaN(data13)){
const err40 = {instancePath:instancePath+"/limits/transferSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/transferSeconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err40];
}
else {
vErrors.push(err40);
}
errors++;
}
}
}
if(data6.transferStallSeconds !== undefined){
let data14 = data6.transferStallSeconds;
if((!((typeof data14 == "number") && (!(data14 % 1) && !isNaN(data14)))) && (data14 !== null)){
const err41 = {instancePath:instancePath+"/limits/transferStallSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/transferStallSeconds/type",keyword:"type",params:{type: schema44.properties.transferStallSeconds.type},message:"must be integer,null"};
if(vErrors === null){
vErrors = [err41];
}
else {
vErrors.push(err41);
}
errors++;
}
if(typeof data14 == "number"){
if(data14 < 0 || isNaN(data14)){
const err42 = {instancePath:instancePath+"/limits/transferStallSeconds",schemaPath:"#/definitions/ExecutionLimits/properties/transferStallSeconds/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err42];
}
else {
vErrors.push(err42);
}
errors++;
}
}
}
}
else {
const err43 = {instancePath:instancePath+"/limits",schemaPath:"#/definitions/ExecutionLimits/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err43];
}
else {
vErrors.push(err43);
}
errors++;
}
}
if(data.protectSystem !== undefined){
if(typeof data.protectSystem !== "boolean"){
const err44 = {instancePath:instancePath+"/protectSystem",schemaPath:"#/properties/protectSystem/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err44];
}
else {
vErrors.push(err44);
}
errors++;
}
}
if(data.protectedPaths !== undefined){
let data16 = data.protectedPaths;
if(Array.isArray(data16)){
const len1 = data16.length;
for(let i1=0; i1<len1; i1++){
if(!(validate35(data16[i1], {instancePath:instancePath+"/protectedPaths/" + i1,parentData:data16,parentDataProperty:i1,rootData}))){
vErrors = vErrors === null ? validate35.errors : vErrors.concat(validate35.errors);
errors = vErrors.length;
}
}
}
else {
const err45 = {instancePath:instancePath+"/protectedPaths",schemaPath:"#/properties/protectedPaths/type",keyword:"type",params:{type: "array"},message:"must be array"};
if(vErrors === null){
vErrors = [err45];
}
else {
vErrors.push(err45);
}
errors++;
}
}
if(data.scriptReview !== undefined){
if(typeof data.scriptReview !== "boolean"){
const err46 = {instancePath:instancePath+"/scriptReview",schemaPath:"#/properties/scriptReview/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err46];
}
else {
vErrors.push(err46);
}
errors++;
}
}
if(data.uploadRoots !== undefined){
let data19 = data.uploadRoots;
if(Array.isArray(data19)){
const len2 = data19.length;
for(let i2=0; i2<len2; i2++){
let data20 = data19[i2];
if(data20 && typeof data20 == "object" && !Array.isArray(data20)){
if(data20.id === undefined){
const err47 = {instancePath:instancePath+"/uploadRoots/" + i2,schemaPath:"#/definitions/LocalDirectoryGrant/required",keyword:"required",params:{missingProperty: "id"},message:"must have required property '"+"id"+"'"};
if(vErrors === null){
vErrors = [err47];
}
else {
vErrors.push(err47);
}
errors++;
}
if(data20.path === undefined){
const err48 = {instancePath:instancePath+"/uploadRoots/" + i2,schemaPath:"#/definitions/LocalDirectoryGrant/required",keyword:"required",params:{missingProperty: "path"},message:"must have required property '"+"path"+"'"};
if(vErrors === null){
vErrors = [err48];
}
else {
vErrors.push(err48);
}
errors++;
}
for(const key3 in data20){
if(!((key3 === "id") || (key3 === "path"))){
const err49 = {instancePath:instancePath+"/uploadRoots/" + i2,schemaPath:"#/definitions/LocalDirectoryGrant/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key3},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err49];
}
else {
vErrors.push(err49);
}
errors++;
}
}
if(data20.id !== undefined){
if(typeof data20.id !== "string"){
const err50 = {instancePath:instancePath+"/uploadRoots/" + i2+"/id",schemaPath:"#/definitions/LocalDirectoryGrant/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err50];
}
else {
vErrors.push(err50);
}
errors++;
}
}
if(data20.path !== undefined){
if(typeof data20.path !== "string"){
const err51 = {instancePath:instancePath+"/uploadRoots/" + i2+"/path",schemaPath:"#/definitions/LocalDirectoryGrant/properties/path/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err51];
}
else {
vErrors.push(err51);
}
errors++;
}
}
}
else {
const err52 = {instancePath:instancePath+"/uploadRoots/" + i2,schemaPath:"#/definitions/LocalDirectoryGrant/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err52];
}
else {
vErrors.push(err52);
}
errors++;
}
}
}
else {
const err53 = {instancePath:instancePath+"/uploadRoots",schemaPath:"#/properties/uploadRoots/type",keyword:"type",params:{type: "array"},message:"must be array"};
if(vErrors === null){
vErrors = [err53];
}
else {
vErrors.push(err53);
}
errors++;
}
}
if(data.version !== undefined){
let data23 = data.version;
if(!((typeof data23 == "number") && (!(data23 % 1) && !isNaN(data23)))){
const err54 = {instancePath:instancePath+"/version",schemaPath:"#/properties/version/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err54];
}
else {
vErrors.push(err54);
}
errors++;
}
if(2 !== data23){
const err55 = {instancePath:instancePath+"/version",schemaPath:"#/properties/version/const",keyword:"const",params:{allowedValue: 2},message:"must be equal to constant"};
if(vErrors === null){
vErrors = [err55];
}
else {
vErrors.push(err55);
}
errors++;
}
if(typeof data23 == "number"){
if(data23 < 0 || isNaN(data23)){
const err56 = {instancePath:instancePath+"/version",schemaPath:"#/properties/version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err56];
}
else {
vErrors.push(err56);
}
errors++;
}
}
}
}
else {
const err57 = {instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err57];
}
else {
vErrors.push(err57);
}
errors++;
}
validate34.errors = vErrors;
return errors === 0;
}


function validate323(data, {instancePath="", parentData, parentDataProperty, rootData=data}={}){
let vErrors = null;
let errors = 0;
if(data && typeof data == "object" && !Array.isArray(data)){
if(data.connectionId === undefined){
const err0 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "connectionId"},message:"must have required property '"+"connectionId"+"'"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
if(data.connectionRevision === undefined){
const err1 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "connectionRevision"},message:"must have required property '"+"connectionRevision"+"'"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
}
if(data.revision === undefined){
const err2 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "revision"},message:"must have required property '"+"revision"+"'"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
if(data.verifyConnect === undefined){
const err3 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "verifyConnect"},message:"must have required property '"+"verifyConnect"+"'"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
if(data.execution === undefined){
const err4 = {instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: "execution"},message:"must have required property '"+"execution"+"'"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
for(const key0 in data){
if(!(((((key0 === "connectionId") || (key0 === "connectionRevision")) || (key0 === "execution")) || (key0 === "revision")) || (key0 === "verifyConnect"))){
const err5 = {instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
}
if(data.connectionId !== undefined){
let data0 = data.connectionId;
if(typeof data0 === "string"){
if(func7(data0) > 128){
const err6 = {instancePath:instancePath+"/connectionId",schemaPath:"#/properties/connectionId/maxLength",keyword:"maxLength",params:{limit: 128},message:"must NOT have more than 128 characters"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
}
if(func7(data0) < 1){
const err7 = {instancePath:instancePath+"/connectionId",schemaPath:"#/properties/connectionId/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
if(!pattern0.test(data0)){
const err8 = {instancePath:instancePath+"/connectionId",schemaPath:"#/properties/connectionId/pattern",keyword:"pattern",params:{pattern: "^[A-Za-z0-9_-]+$"},message:"must match pattern \""+"^[A-Za-z0-9_-]+$"+"\""};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
}
else {
const err9 = {instancePath:instancePath+"/connectionId",schemaPath:"#/properties/connectionId/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
}
if(data.connectionRevision !== undefined){
let data1 = data.connectionRevision;
if(!((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1)))){
const err10 = {instancePath:instancePath+"/connectionRevision",schemaPath:"#/properties/connectionRevision/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
if(typeof data1 == "number"){
if(data1 > 9007199254740991 || isNaN(data1)){
const err11 = {instancePath:instancePath+"/connectionRevision",schemaPath:"#/properties/connectionRevision/maximum",keyword:"maximum",params:{comparison: "<=", limit: 9007199254740991},message:"must be <= 9007199254740991"};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
}
if(data1 < 0 || isNaN(data1)){
const err12 = {instancePath:instancePath+"/connectionRevision",schemaPath:"#/properties/connectionRevision/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err12];
}
else {
vErrors.push(err12);
}
errors++;
}
}
}
if(data.execution !== undefined){
if(!(validate34(data.execution, {instancePath:instancePath+"/execution",parentData:data,parentDataProperty:"execution",rootData}))){
vErrors = vErrors === null ? validate34.errors : vErrors.concat(validate34.errors);
errors = vErrors.length;
}
}
if(data.revision !== undefined){
let data3 = data.revision;
if(!((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3)))){
const err13 = {instancePath:instancePath+"/revision",schemaPath:"#/properties/revision/type",keyword:"type",params:{type: "integer"},message:"must be integer"};
if(vErrors === null){
vErrors = [err13];
}
else {
vErrors.push(err13);
}
errors++;
}
if(typeof data3 == "number"){
if(data3 > 9007199254740991 || isNaN(data3)){
const err14 = {instancePath:instancePath+"/revision",schemaPath:"#/properties/revision/maximum",keyword:"maximum",params:{comparison: "<=", limit: 9007199254740991},message:"must be <= 9007199254740991"};
if(vErrors === null){
vErrors = [err14];
}
else {
vErrors.push(err14);
}
errors++;
}
if(data3 < 0 || isNaN(data3)){
const err15 = {instancePath:instancePath+"/revision",schemaPath:"#/properties/revision/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"};
if(vErrors === null){
vErrors = [err15];
}
else {
vErrors.push(err15);
}
errors++;
}
}
}
if(data.verifyConnect !== undefined){
if(typeof data.verifyConnect !== "boolean"){
const err16 = {instancePath:instancePath+"/verifyConnect",schemaPath:"#/properties/verifyConnect/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"};
if(vErrors === null){
vErrors = [err16];
}
else {
vErrors.push(err16);
}
errors++;
}
}
}
else {
const err17 = {instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err17];
}
else {
vErrors.push(err17);
}
errors++;
}
validate323.errors = vErrors;
return errors === 0;
}
