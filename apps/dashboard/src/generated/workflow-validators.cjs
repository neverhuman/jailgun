/** Generated from Rust schemas by scripts/generate-contracts.mjs. DO NOT EDIT. */
"use strict";
exports.account_readiness = validate127;
const schema20 = {"properties":{"active":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"capacity":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"cooldown_until_ms":{"format":"int64","type":"integer"},"id":{"type":"string"},"next_submit_ms":{"format":"int64","type":"integer"},"rate_limit_count":{"format":"uint32","minimum":0,"type":"integer"},"readiness":{"type":"string"}},"required":["id","readiness","capacity","active","next_submit_ms","cooldown_until_ms","rate_limit_count"],"type":"object"};

function validate127(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate127.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((data.id === undefined) && (missing0 = "id")) || ((data.readiness === undefined) && (missing0 = "readiness"))) || ((data.capacity === undefined) && (missing0 = "capacity"))) || ((data.active === undefined) && (missing0 = "active"))) || ((data.next_submit_ms === undefined) && (missing0 = "next_submit_ms"))) || ((data.cooldown_until_ms === undefined) && (missing0 = "cooldown_until_ms"))) || ((data.rate_limit_count === undefined) && (missing0 = "rate_limit_count"))){
validate127.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.active !== undefined){
let data0 = data.active;
const _errs1 = errors;
if(!(((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0))) && (isFinite(data0)))){
validate127.errors = [{instancePath:instancePath+"/active",schemaPath:"#/properties/active/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs1){
if((typeof data0 == "number") && (isFinite(data0))){
if(data0 > 65535 || isNaN(data0)){
validate127.errors = [{instancePath:instancePath+"/active",schemaPath:"#/properties/active/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data0 < 0 || isNaN(data0)){
validate127.errors = [{instancePath:instancePath+"/active",schemaPath:"#/properties/active/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.capacity !== undefined){
let data1 = data.capacity;
const _errs3 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate127.errors = [{instancePath:instancePath+"/capacity",schemaPath:"#/properties/capacity/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs3){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 > 65535 || isNaN(data1)){
validate127.errors = [{instancePath:instancePath+"/capacity",schemaPath:"#/properties/capacity/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data1 < 0 || isNaN(data1)){
validate127.errors = [{instancePath:instancePath+"/capacity",schemaPath:"#/properties/capacity/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.cooldown_until_ms !== undefined){
let data2 = data.cooldown_until_ms;
const _errs5 = errors;
if(!(((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2))) && (isFinite(data2)))){
validate127.errors = [{instancePath:instancePath+"/cooldown_until_ms",schemaPath:"#/properties/cooldown_until_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs7 = errors;
if(typeof data.id !== "string"){
validate127.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_submit_ms !== undefined){
let data4 = data.next_submit_ms;
const _errs9 = errors;
if(!(((typeof data4 == "number") && (!(data4 % 1) && !isNaN(data4))) && (isFinite(data4)))){
validate127.errors = [{instancePath:instancePath+"/next_submit_ms",schemaPath:"#/properties/next_submit_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.rate_limit_count !== undefined){
let data5 = data.rate_limit_count;
const _errs11 = errors;
if(!(((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5))) && (isFinite(data5)))){
validate127.errors = [{instancePath:instancePath+"/rate_limit_count",schemaPath:"#/properties/rate_limit_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs11){
if((typeof data5 == "number") && (isFinite(data5))){
if(data5 < 0 || isNaN(data5)){
validate127.errors = [{instancePath:instancePath+"/rate_limit_count",schemaPath:"#/properties/rate_limit_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.readiness !== undefined){
const _errs13 = errors;
if(typeof data.readiness !== "string"){
validate127.errors = [{instancePath:instancePath+"/readiness",schemaPath:"#/properties/readiness/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
else {
validate127.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate127.errors = vErrors;
return errors === 0;
}
validate127.evaluated = {"props":{"active":true,"capacity":true,"cooldown_until_ms":true,"id":true,"next_submit_ms":true,"rate_limit_count":true,"readiness":true},"dynamicProps":false,"dynamicItems":false};

exports.account_session = validate128;
const schema21 = {"properties":{"account_id":{"type":"string"},"email":{"type":"string"},"error_code":{"type":["string","null"]},"lifecycle":{"type":"string"},"login_expires_ms":{"format":"int64","type":["integer","null"]},"login_view_available":{"default":false,"description":"Ephemeral supervisor observation; never persisted as account readiness.","type":"boolean"},"model":{"anyOf":[{"$ref":"#/$defs/ModelSelection"},{"type":"null"}]},"observation":{"anyOf":[{"$ref":"#/$defs/AccountObservation"},{"type":"null"}]},"observed_ms":{"format":"int64","type":["integer","null"]}},"required":["account_id","email","lifecycle"],"type":"object"};
const schema22 = {"oneOf":[{"additionalProperties":false,"properties":{"mode":{"const":"current","type":"string"}},"required":["mode"],"type":"object"},{"additionalProperties":false,"properties":{"mode":{"const":"specific","type":"string"},"name":{"type":"string"}},"required":["mode","name"],"type":"object"}]};

function validate56(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate56.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
const _errs0 = errors;
let valid0 = false;
let passing0 = null;
const _errs1 = errors;
if(errors === _errs1){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((data.mode === undefined) && (missing0 = "mode")){
const err0 = {instancePath,schemaPath:"#/oneOf/0/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
else {
const _errs3 = errors;
for(const key0 in data){
if(!(key0 === "mode")){
const err1 = {instancePath,schemaPath:"#/oneOf/0/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
break;
}
}
if(_errs3 === errors){
if(data.mode !== undefined){
let data0 = data.mode;
if(typeof data0 !== "string"){
const err2 = {instancePath:instancePath+"/mode",schemaPath:"#/oneOf/0/properties/mode/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
if("current" !== data0){
const err3 = {instancePath:instancePath+"/mode",schemaPath:"#/oneOf/0/properties/mode/const",keyword:"const",params:{allowedValue: "current"},message:"must be equal to constant"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
}
}
}
}
}
else {
const err4 = {instancePath,schemaPath:"#/oneOf/0/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err4];
}
else {
vErrors.push(err4);
}
errors++;
}
}
var _valid0 = _errs1 === errors;
if(_valid0){
valid0 = true;
passing0 = 0;
var props0 = true;
}
const _errs6 = errors;
if(errors === _errs6){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing1;
if(((data.mode === undefined) && (missing1 = "mode")) || ((data.name === undefined) && (missing1 = "name"))){
const err5 = {instancePath,schemaPath:"#/oneOf/1/required",keyword:"required",params:{missingProperty: missing1},message:"must have required property '"+missing1+"'"};
if(vErrors === null){
vErrors = [err5];
}
else {
vErrors.push(err5);
}
errors++;
}
else {
const _errs8 = errors;
for(const key1 in data){
if(!((key1 === "mode") || (key1 === "name"))){
const err6 = {instancePath,schemaPath:"#/oneOf/1/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key1},message:"must NOT have additional properties"};
if(vErrors === null){
vErrors = [err6];
}
else {
vErrors.push(err6);
}
errors++;
break;
}
}
if(_errs8 === errors){
if(data.mode !== undefined){
let data1 = data.mode;
const _errs9 = errors;
if(typeof data1 !== "string"){
const err7 = {instancePath:instancePath+"/mode",schemaPath:"#/oneOf/1/properties/mode/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err7];
}
else {
vErrors.push(err7);
}
errors++;
}
if("specific" !== data1){
const err8 = {instancePath:instancePath+"/mode",schemaPath:"#/oneOf/1/properties/mode/const",keyword:"const",params:{allowedValue: "specific"},message:"must be equal to constant"};
if(vErrors === null){
vErrors = [err8];
}
else {
vErrors.push(err8);
}
errors++;
}
var valid2 = _errs9 === errors;
}
else {
var valid2 = true;
}
if(valid2){
if(data.name !== undefined){
const _errs11 = errors;
if(typeof data.name !== "string"){
const err9 = {instancePath:instancePath+"/name",schemaPath:"#/oneOf/1/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"};
if(vErrors === null){
vErrors = [err9];
}
else {
vErrors.push(err9);
}
errors++;
}
var valid2 = _errs11 === errors;
}
else {
var valid2 = true;
}
}
}
}
}
else {
const err10 = {instancePath,schemaPath:"#/oneOf/1/type",keyword:"type",params:{type: "object"},message:"must be object"};
if(vErrors === null){
vErrors = [err10];
}
else {
vErrors.push(err10);
}
errors++;
}
}
var _valid0 = _errs6 === errors;
if(_valid0 && valid0){
valid0 = false;
passing0 = [passing0, 1];
}
else {
if(_valid0){
valid0 = true;
passing0 = 1;
if(props0 !== true){
props0 = true;
}
}
}
if(!valid0){
const err11 = {instancePath,schemaPath:"#/oneOf",keyword:"oneOf",params:{passingSchemas: passing0},message:"must match exactly one schema in oneOf"};
if(vErrors === null){
vErrors = [err11];
}
else {
vErrors.push(err11);
}
errors++;
validate56.errors = vErrors;
return false;
}
else {
errors = _errs0;
if(vErrors !== null){
if(_errs0){
vErrors.length = _errs0;
}
else {
vErrors = null;
}
}
}
validate56.errors = vErrors;
evaluated0.props = props0;
return errors === 0;
}
validate56.evaluated = {"dynamicProps":true,"dynamicItems":false};

const schema23 = {"properties":{"available_models":{"items":{"type":"string"},"type":"array"},"identity":{"$ref":"#/$defs/AccountIdentity"},"model":{"type":"string"}},"required":["identity","model","available_models"],"type":"object"};
const schema24 = {"properties":{"email":{"type":"string"},"id":{"type":"string"}},"required":["id","email"],"type":"object"};

function validate59(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate59.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.id === undefined) && (missing0 = "id")) || ((data.email === undefined) && (missing0 = "email"))){
validate59.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.email !== undefined){
const _errs1 = errors;
if(typeof data.email !== "string"){
validate59.errors = [{instancePath:instancePath+"/email",schemaPath:"#/properties/email/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs3 = errors;
if(typeof data.id !== "string"){
validate59.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
}
}
}
else {
validate59.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate59.errors = vErrors;
return errors === 0;
}
validate59.evaluated = {"props":{"email":true,"id":true},"dynamicProps":false,"dynamicItems":false};


function validate58(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate58.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.identity === undefined) && (missing0 = "identity")) || ((data.model === undefined) && (missing0 = "model"))) || ((data.available_models === undefined) && (missing0 = "available_models"))){
validate58.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.available_models !== undefined){
let data0 = data.available_models;
const _errs1 = errors;
if(errors === _errs1){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs3 = errors;
if(typeof data0[i0] !== "string"){
validate58.errors = [{instancePath:instancePath+"/available_models/" + i0,schemaPath:"#/properties/available_models/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs3 === errors;
if(!valid1){
break;
}
}
}
else {
validate58.errors = [{instancePath:instancePath+"/available_models",schemaPath:"#/properties/available_models/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.identity !== undefined){
const _errs5 = errors;
if(!(validate59(data.identity, {instancePath:instancePath+"/identity",parentData:data,parentDataProperty:"identity",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate59.errors : vErrors.concat(validate59.errors);
errors = vErrors.length;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.model !== undefined){
const _errs6 = errors;
if(typeof data.model !== "string"){
validate58.errors = [{instancePath:instancePath+"/model",schemaPath:"#/properties/model/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate58.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate58.errors = vErrors;
return errors === 0;
}
validate58.evaluated = {"props":{"available_models":true,"identity":true,"model":true},"dynamicProps":false,"dynamicItems":false};


function validate128(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate128.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.account_id === undefined) && (missing0 = "account_id")) || ((data.email === undefined) && (missing0 = "email"))) || ((data.lifecycle === undefined) && (missing0 = "lifecycle"))){
validate128.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.account_id !== undefined){
const _errs1 = errors;
if(typeof data.account_id !== "string"){
validate128.errors = [{instancePath:instancePath+"/account_id",schemaPath:"#/properties/account_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.email !== undefined){
const _errs3 = errors;
if(typeof data.email !== "string"){
validate128.errors = [{instancePath:instancePath+"/email",schemaPath:"#/properties/email/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.error_code !== undefined){
let data2 = data.error_code;
const _errs5 = errors;
if((typeof data2 !== "string") && (data2 !== null)){
validate128.errors = [{instancePath:instancePath+"/error_code",schemaPath:"#/properties/error_code/type",keyword:"type",params:{type: schema21.properties.error_code.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.lifecycle !== undefined){
const _errs7 = errors;
if(typeof data.lifecycle !== "string"){
validate128.errors = [{instancePath:instancePath+"/lifecycle",schemaPath:"#/properties/lifecycle/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.login_expires_ms !== undefined){
let data4 = data.login_expires_ms;
const _errs9 = errors;
if((!(((typeof data4 == "number") && (!(data4 % 1) && !isNaN(data4))) && (isFinite(data4)))) && (data4 !== null)){
validate128.errors = [{instancePath:instancePath+"/login_expires_ms",schemaPath:"#/properties/login_expires_ms/type",keyword:"type",params:{type: schema21.properties.login_expires_ms.type},message:"must be integer,null"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.login_view_available !== undefined){
const _errs11 = errors;
if(typeof data.login_view_available !== "boolean"){
validate128.errors = [{instancePath:instancePath+"/login_view_available",schemaPath:"#/properties/login_view_available/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.model !== undefined){
let data6 = data.model;
const _errs13 = errors;
const _errs14 = errors;
let valid1 = false;
const _errs15 = errors;
if(!(validate56(data6, {instancePath:instancePath+"/model",parentData:data,parentDataProperty:"model",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate56.errors : vErrors.concat(validate56.errors);
errors = vErrors.length;
}
var _valid0 = _errs15 === errors;
valid1 = valid1 || _valid0;
const _errs16 = errors;
if(data6 !== null){
const err0 = {instancePath:instancePath+"/model",schemaPath:"#/properties/model/anyOf/1/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs16 === errors;
valid1 = valid1 || _valid0;
if(!valid1){
const err1 = {instancePath:instancePath+"/model",schemaPath:"#/properties/model/anyOf",keyword:"anyOf",params:{},message:"must match a schema in anyOf"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
validate128.errors = vErrors;
return false;
}
else {
errors = _errs14;
if(vErrors !== null){
if(_errs14){
vErrors.length = _errs14;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.observation !== undefined){
let data7 = data.observation;
const _errs18 = errors;
const _errs19 = errors;
let valid2 = false;
const _errs20 = errors;
if(!(validate58(data7, {instancePath:instancePath+"/observation",parentData:data,parentDataProperty:"observation",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate58.errors : vErrors.concat(validate58.errors);
errors = vErrors.length;
}
var _valid1 = _errs20 === errors;
valid2 = valid2 || _valid1;
if(_valid1){
var props1 = {};
props1.available_models = true;
props1.identity = true;
props1.model = true;
}
const _errs21 = errors;
if(data7 !== null){
const err2 = {instancePath:instancePath+"/observation",schemaPath:"#/properties/observation/anyOf/1/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err2];
}
else {
vErrors.push(err2);
}
errors++;
}
var _valid1 = _errs21 === errors;
valid2 = valid2 || _valid1;
if(!valid2){
const err3 = {instancePath:instancePath+"/observation",schemaPath:"#/properties/observation/anyOf",keyword:"anyOf",params:{},message:"must match a schema in anyOf"};
if(vErrors === null){
vErrors = [err3];
}
else {
vErrors.push(err3);
}
errors++;
validate128.errors = vErrors;
return false;
}
else {
errors = _errs19;
if(vErrors !== null){
if(_errs19){
vErrors.length = _errs19;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs18 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.observed_ms !== undefined){
let data8 = data.observed_ms;
const _errs23 = errors;
if((!(((typeof data8 == "number") && (!(data8 % 1) && !isNaN(data8))) && (isFinite(data8)))) && (data8 !== null)){
validate128.errors = [{instancePath:instancePath+"/observed_ms",schemaPath:"#/properties/observed_ms/type",keyword:"type",params:{type: schema21.properties.observed_ms.type},message:"must be integer,null"}];
return false;
}
var valid0 = _errs23 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate128.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate128.errors = vErrors;
return errors === 0;
}
validate128.evaluated = {"props":{"account_id":true,"email":true,"error_code":true,"lifecycle":true,"login_expires_ms":true,"login_view_available":true,"model":true,"observation":true,"observed_ms":true},"dynamicProps":false,"dynamicItems":false};

exports.artifact_chunk = validate131;
const schema25 = {"properties":{"artifact":{"$ref":"#/$defs/Artifact"},"eof":{"type":"boolean"},"next_offset":{"format":"uint64","minimum":0,"type":["integer","null"]},"offset":{"format":"uint64","minimum":0,"type":"integer"},"text":{"type":"string"}},"required":["artifact","offset","eof","text"],"type":"object"};
const schema26 = {"properties":{"attempt_id":{"type":["string","null"]},"byte_length":{"format":"uint64","minimum":0,"type":"integer"},"completion":{"type":"string"},"created_ms":{"format":"int64","type":"integer"},"id":{"type":"string"},"media_type":{"type":"string"},"name":{"type":"string"},"run_id":{"type":"string"},"sha256":{"type":"string"}},"required":["id","run_id","name","media_type","sha256","byte_length","completion","created_ms"],"type":"object"};

function validate64(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate64.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((((((data.id === undefined) && (missing0 = "id")) || ((data.run_id === undefined) && (missing0 = "run_id"))) || ((data.name === undefined) && (missing0 = "name"))) || ((data.media_type === undefined) && (missing0 = "media_type"))) || ((data.sha256 === undefined) && (missing0 = "sha256"))) || ((data.byte_length === undefined) && (missing0 = "byte_length"))) || ((data.completion === undefined) && (missing0 = "completion"))) || ((data.created_ms === undefined) && (missing0 = "created_ms"))){
validate64.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.attempt_id !== undefined){
let data0 = data.attempt_id;
const _errs1 = errors;
if((typeof data0 !== "string") && (data0 !== null)){
validate64.errors = [{instancePath:instancePath+"/attempt_id",schemaPath:"#/properties/attempt_id/type",keyword:"type",params:{type: schema26.properties.attempt_id.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.byte_length !== undefined){
let data1 = data.byte_length;
const _errs3 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate64.errors = [{instancePath:instancePath+"/byte_length",schemaPath:"#/properties/byte_length/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs3){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 < 0 || isNaN(data1)){
validate64.errors = [{instancePath:instancePath+"/byte_length",schemaPath:"#/properties/byte_length/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.completion !== undefined){
const _errs5 = errors;
if(typeof data.completion !== "string"){
validate64.errors = [{instancePath:instancePath+"/completion",schemaPath:"#/properties/completion/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_ms !== undefined){
let data3 = data.created_ms;
const _errs7 = errors;
if(!(((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3))) && (isFinite(data3)))){
validate64.errors = [{instancePath:instancePath+"/created_ms",schemaPath:"#/properties/created_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs9 = errors;
if(typeof data.id !== "string"){
validate64.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.media_type !== undefined){
const _errs11 = errors;
if(typeof data.media_type !== "string"){
validate64.errors = [{instancePath:instancePath+"/media_type",schemaPath:"#/properties/media_type/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.name !== undefined){
const _errs13 = errors;
if(typeof data.name !== "string"){
validate64.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
const _errs15 = errors;
if(typeof data.run_id !== "string"){
validate64.errors = [{instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs15 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.sha256 !== undefined){
const _errs17 = errors;
if(typeof data.sha256 !== "string"){
validate64.errors = [{instancePath:instancePath+"/sha256",schemaPath:"#/properties/sha256/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs17 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate64.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate64.errors = vErrors;
return errors === 0;
}
validate64.evaluated = {"props":{"attempt_id":true,"byte_length":true,"completion":true,"created_ms":true,"id":true,"media_type":true,"name":true,"run_id":true,"sha256":true},"dynamicProps":false,"dynamicItems":false};


function validate131(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate131.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.artifact === undefined) && (missing0 = "artifact")) || ((data.offset === undefined) && (missing0 = "offset"))) || ((data.eof === undefined) && (missing0 = "eof"))) || ((data.text === undefined) && (missing0 = "text"))){
validate131.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.artifact !== undefined){
const _errs1 = errors;
if(!(validate64(data.artifact, {instancePath:instancePath+"/artifact",parentData:data,parentDataProperty:"artifact",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate64.errors : vErrors.concat(validate64.errors);
errors = vErrors.length;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.eof !== undefined){
const _errs2 = errors;
if(typeof data.eof !== "boolean"){
validate131.errors = [{instancePath:instancePath+"/eof",schemaPath:"#/properties/eof/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_offset !== undefined){
let data2 = data.next_offset;
const _errs4 = errors;
if((!(((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2))) && (isFinite(data2)))) && (data2 !== null)){
validate131.errors = [{instancePath:instancePath+"/next_offset",schemaPath:"#/properties/next_offset/type",keyword:"type",params:{type: schema25.properties.next_offset.type},message:"must be integer,null"}];
return false;
}
if(errors === _errs4){
if((typeof data2 == "number") && (isFinite(data2))){
if(data2 < 0 || isNaN(data2)){
validate131.errors = [{instancePath:instancePath+"/next_offset",schemaPath:"#/properties/next_offset/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.offset !== undefined){
let data3 = data.offset;
const _errs6 = errors;
if(!(((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3))) && (isFinite(data3)))){
validate131.errors = [{instancePath:instancePath+"/offset",schemaPath:"#/properties/offset/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs6){
if((typeof data3 == "number") && (isFinite(data3))){
if(data3 < 0 || isNaN(data3)){
validate131.errors = [{instancePath:instancePath+"/offset",schemaPath:"#/properties/offset/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.text !== undefined){
const _errs8 = errors;
if(typeof data.text !== "string"){
validate131.errors = [{instancePath:instancePath+"/text",schemaPath:"#/properties/text/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate131.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate131.errors = vErrors;
return errors === 0;
}
validate131.evaluated = {"props":{"artifact":true,"eof":true,"next_offset":true,"offset":true,"text":true},"dynamicProps":false,"dynamicItems":false};

exports.artifact_read = validate133;
const schema27 = {"additionalProperties":false,"properties":{"artifact_id":{"type":"string"},"limit":{"default":16384,"format":"uint32","maximum":16384,"minimum":4,"type":"integer"},"offset":{"default":0,"description":"UTF-8 byte offset, always use next_offset from the preceding response.","format":"uint64","minimum":0,"type":"integer"},"run_id":{"type":"string"}},"required":["run_id","artifact_id"],"type":"object"};

function validate133(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate133.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.run_id === undefined) && (missing0 = "run_id")) || ((data.artifact_id === undefined) && (missing0 = "artifact_id"))){
validate133.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((key0 === "artifact_id") || (key0 === "limit")) || (key0 === "offset")) || (key0 === "run_id"))){
validate133.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.artifact_id !== undefined){
const _errs2 = errors;
if(typeof data.artifact_id !== "string"){
validate133.errors = [{instancePath:instancePath+"/artifact_id",schemaPath:"#/properties/artifact_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.limit !== undefined){
let data1 = data.limit;
const _errs4 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate133.errors = [{instancePath:instancePath+"/limit",schemaPath:"#/properties/limit/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs4){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 > 16384 || isNaN(data1)){
validate133.errors = [{instancePath:instancePath+"/limit",schemaPath:"#/properties/limit/maximum",keyword:"maximum",params:{comparison: "<=", limit: 16384},message:"must be <= 16384"}];
return false;
}
else {
if(data1 < 4 || isNaN(data1)){
validate133.errors = [{instancePath:instancePath+"/limit",schemaPath:"#/properties/limit/minimum",keyword:"minimum",params:{comparison: ">=", limit: 4},message:"must be >= 4"}];
return false;
}
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.offset !== undefined){
let data2 = data.offset;
const _errs6 = errors;
if(!(((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2))) && (isFinite(data2)))){
validate133.errors = [{instancePath:instancePath+"/offset",schemaPath:"#/properties/offset/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs6){
if((typeof data2 == "number") && (isFinite(data2))){
if(data2 < 0 || isNaN(data2)){
validate133.errors = [{instancePath:instancePath+"/offset",schemaPath:"#/properties/offset/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
const _errs8 = errors;
if(typeof data.run_id !== "string"){
validate133.errors = [{instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate133.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate133.errors = vErrors;
return errors === 0;
}
validate133.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.attempt = validate134;
const schema28 = {"properties":{"accepted_ms":{"format":"int64","type":["integer","null"]},"completed_ms":{"format":"int64","type":["integer","null"]},"conversation_id":{"type":["string","null"]},"conversation_url":{"type":["string","null"]},"created_ms":{"format":"int64","type":"integer"},"error_code":{"type":["string","null"]},"id":{"type":"string"},"number":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"observed_model":{"type":["string","null"]},"run_id":{"type":"string"},"state":{"type":"string"},"task_id":{"type":"string"},"user_turn_id":{"type":["string","null"]}},"required":["id","run_id","task_id","number","state","created_ms"],"type":"object"};

function validate134(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate134.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((((data.id === undefined) && (missing0 = "id")) || ((data.run_id === undefined) && (missing0 = "run_id"))) || ((data.task_id === undefined) && (missing0 = "task_id"))) || ((data.number === undefined) && (missing0 = "number"))) || ((data.state === undefined) && (missing0 = "state"))) || ((data.created_ms === undefined) && (missing0 = "created_ms"))){
validate134.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.accepted_ms !== undefined){
let data0 = data.accepted_ms;
const _errs1 = errors;
if((!(((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0))) && (isFinite(data0)))) && (data0 !== null)){
validate134.errors = [{instancePath:instancePath+"/accepted_ms",schemaPath:"#/properties/accepted_ms/type",keyword:"type",params:{type: schema28.properties.accepted_ms.type},message:"must be integer,null"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.completed_ms !== undefined){
let data1 = data.completed_ms;
const _errs3 = errors;
if((!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))) && (data1 !== null)){
validate134.errors = [{instancePath:instancePath+"/completed_ms",schemaPath:"#/properties/completed_ms/type",keyword:"type",params:{type: schema28.properties.completed_ms.type},message:"must be integer,null"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.conversation_id !== undefined){
let data2 = data.conversation_id;
const _errs5 = errors;
if((typeof data2 !== "string") && (data2 !== null)){
validate134.errors = [{instancePath:instancePath+"/conversation_id",schemaPath:"#/properties/conversation_id/type",keyword:"type",params:{type: schema28.properties.conversation_id.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.conversation_url !== undefined){
let data3 = data.conversation_url;
const _errs7 = errors;
if((typeof data3 !== "string") && (data3 !== null)){
validate134.errors = [{instancePath:instancePath+"/conversation_url",schemaPath:"#/properties/conversation_url/type",keyword:"type",params:{type: schema28.properties.conversation_url.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_ms !== undefined){
let data4 = data.created_ms;
const _errs9 = errors;
if(!(((typeof data4 == "number") && (!(data4 % 1) && !isNaN(data4))) && (isFinite(data4)))){
validate134.errors = [{instancePath:instancePath+"/created_ms",schemaPath:"#/properties/created_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.error_code !== undefined){
let data5 = data.error_code;
const _errs11 = errors;
if((typeof data5 !== "string") && (data5 !== null)){
validate134.errors = [{instancePath:instancePath+"/error_code",schemaPath:"#/properties/error_code/type",keyword:"type",params:{type: schema28.properties.error_code.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs13 = errors;
if(typeof data.id !== "string"){
validate134.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.number !== undefined){
let data7 = data.number;
const _errs15 = errors;
if(!(((typeof data7 == "number") && (!(data7 % 1) && !isNaN(data7))) && (isFinite(data7)))){
validate134.errors = [{instancePath:instancePath+"/number",schemaPath:"#/properties/number/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs15){
if((typeof data7 == "number") && (isFinite(data7))){
if(data7 > 65535 || isNaN(data7)){
validate134.errors = [{instancePath:instancePath+"/number",schemaPath:"#/properties/number/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data7 < 0 || isNaN(data7)){
validate134.errors = [{instancePath:instancePath+"/number",schemaPath:"#/properties/number/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs15 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.observed_model !== undefined){
let data8 = data.observed_model;
const _errs17 = errors;
if((typeof data8 !== "string") && (data8 !== null)){
validate134.errors = [{instancePath:instancePath+"/observed_model",schemaPath:"#/properties/observed_model/type",keyword:"type",params:{type: schema28.properties.observed_model.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs17 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
const _errs19 = errors;
if(typeof data.run_id !== "string"){
validate134.errors = [{instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.state !== undefined){
const _errs21 = errors;
if(typeof data.state !== "string"){
validate134.errors = [{instancePath:instancePath+"/state",schemaPath:"#/properties/state/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs21 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.task_id !== undefined){
const _errs23 = errors;
if(typeof data.task_id !== "string"){
validate134.errors = [{instancePath:instancePath+"/task_id",schemaPath:"#/properties/task_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs23 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.user_turn_id !== undefined){
let data12 = data.user_turn_id;
const _errs25 = errors;
if((typeof data12 !== "string") && (data12 !== null)){
validate134.errors = [{instancePath:instancePath+"/user_turn_id",schemaPath:"#/properties/user_turn_id/type",keyword:"type",params:{type: schema28.properties.user_turn_id.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs25 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate134.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate134.errors = vErrors;
return errors === 0;
}
validate134.evaluated = {"props":{"accepted_ms":true,"completed_ms":true,"conversation_id":true,"conversation_url":true,"created_ms":true,"error_code":true,"id":true,"number":true,"observed_model":true,"run_id":true,"state":true,"task_id":true,"user_turn_id":true},"dynamicProps":false,"dynamicItems":false};

exports.comparison = validate135;
const schema29 = {"properties":{"basis":{"type":"string"},"ranking":{"items":{"$ref":"#/$defs/RankedCandidate"},"type":"array"}},"required":["basis","ranking"],"type":"object"};
const schema30 = {"properties":{"evaluation":{"$ref":"#/$defs/CandidateEvaluation"},"weighted_score":{"format":"double","type":"number"}},"required":["weighted_score","evaluation"],"type":"object"};
const schema31 = {"additionalProperties":false,"properties":{"candidate":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"disagreements":{"items":{"type":"string"},"type":"array"},"scores":{"items":{"$ref":"#/$defs/CriterionScore"},"type":"array"},"uncertainty":{"type":"string"}},"required":["candidate","scores","disagreements","uncertainty"],"type":"object"};
const schema32 = {"additionalProperties":false,"properties":{"criterion":{"type":"string"},"rationale":{"type":"string"},"score":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"}},"required":["criterion","score","rationale"],"type":"object"};

function validate74(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate74.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.criterion === undefined) && (missing0 = "criterion")) || ((data.score === undefined) && (missing0 = "score"))) || ((data.rationale === undefined) && (missing0 = "rationale"))){
validate74.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((key0 === "criterion") || (key0 === "rationale")) || (key0 === "score"))){
validate74.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.criterion !== undefined){
const _errs2 = errors;
if(typeof data.criterion !== "string"){
validate74.errors = [{instancePath:instancePath+"/criterion",schemaPath:"#/properties/criterion/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.rationale !== undefined){
const _errs4 = errors;
if(typeof data.rationale !== "string"){
validate74.errors = [{instancePath:instancePath+"/rationale",schemaPath:"#/properties/rationale/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.score !== undefined){
let data2 = data.score;
const _errs6 = errors;
if(!(((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2))) && (isFinite(data2)))){
validate74.errors = [{instancePath:instancePath+"/score",schemaPath:"#/properties/score/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs6){
if((typeof data2 == "number") && (isFinite(data2))){
if(data2 > 65535 || isNaN(data2)){
validate74.errors = [{instancePath:instancePath+"/score",schemaPath:"#/properties/score/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data2 < 0 || isNaN(data2)){
validate74.errors = [{instancePath:instancePath+"/score",schemaPath:"#/properties/score/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate74.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate74.errors = vErrors;
return errors === 0;
}
validate74.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate73(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate73.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.candidate === undefined) && (missing0 = "candidate")) || ((data.scores === undefined) && (missing0 = "scores"))) || ((data.disagreements === undefined) && (missing0 = "disagreements"))) || ((data.uncertainty === undefined) && (missing0 = "uncertainty"))){
validate73.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((key0 === "candidate") || (key0 === "disagreements")) || (key0 === "scores")) || (key0 === "uncertainty"))){
validate73.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.candidate !== undefined){
let data0 = data.candidate;
const _errs2 = errors;
if(!(((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0))) && (isFinite(data0)))){
validate73.errors = [{instancePath:instancePath+"/candidate",schemaPath:"#/properties/candidate/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs2){
if((typeof data0 == "number") && (isFinite(data0))){
if(data0 > 65535 || isNaN(data0)){
validate73.errors = [{instancePath:instancePath+"/candidate",schemaPath:"#/properties/candidate/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data0 < 0 || isNaN(data0)){
validate73.errors = [{instancePath:instancePath+"/candidate",schemaPath:"#/properties/candidate/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.disagreements !== undefined){
let data1 = data.disagreements;
const _errs4 = errors;
if(errors === _errs4){
if(Array.isArray(data1)){
var valid1 = true;
const len0 = data1.length;
for(let i0=0; i0<len0; i0++){
const _errs6 = errors;
if(typeof data1[i0] !== "string"){
validate73.errors = [{instancePath:instancePath+"/disagreements/" + i0,schemaPath:"#/properties/disagreements/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs6 === errors;
if(!valid1){
break;
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/disagreements",schemaPath:"#/properties/disagreements/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.scores !== undefined){
let data3 = data.scores;
const _errs8 = errors;
if(errors === _errs8){
if(Array.isArray(data3)){
var valid2 = true;
const len1 = data3.length;
for(let i1=0; i1<len1; i1++){
const _errs10 = errors;
if(!(validate74(data3[i1], {instancePath:instancePath+"/scores/" + i1,parentData:data3,parentDataProperty:i1,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate74.errors : vErrors.concat(validate74.errors);
errors = vErrors.length;
}
var valid2 = _errs10 === errors;
if(!valid2){
break;
}
}
}
else {
validate73.errors = [{instancePath:instancePath+"/scores",schemaPath:"#/properties/scores/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.uncertainty !== undefined){
const _errs11 = errors;
if(typeof data.uncertainty !== "string"){
validate73.errors = [{instancePath:instancePath+"/uncertainty",schemaPath:"#/properties/uncertainty/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate73.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate73.errors = vErrors;
return errors === 0;
}
validate73.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate72(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate72.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.weighted_score === undefined) && (missing0 = "weighted_score")) || ((data.evaluation === undefined) && (missing0 = "evaluation"))){
validate72.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.evaluation !== undefined){
const _errs1 = errors;
if(!(validate73(data.evaluation, {instancePath:instancePath+"/evaluation",parentData:data,parentDataProperty:"evaluation",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate73.errors : vErrors.concat(validate73.errors);
errors = vErrors.length;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.weighted_score !== undefined){
let data1 = data.weighted_score;
const _errs2 = errors;
if(errors === _errs2){
if(!((typeof data1 == "number") && (isFinite(data1)))){
validate72.errors = [{instancePath:instancePath+"/weighted_score",schemaPath:"#/properties/weighted_score/type",keyword:"type",params:{type: "number"},message:"must be number"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
}
}
}
else {
validate72.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate72.errors = vErrors;
return errors === 0;
}
validate72.evaluated = {"props":{"evaluation":true,"weighted_score":true},"dynamicProps":false,"dynamicItems":false};


function validate135(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate135.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.basis === undefined) && (missing0 = "basis")) || ((data.ranking === undefined) && (missing0 = "ranking"))){
validate135.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.basis !== undefined){
const _errs1 = errors;
if(typeof data.basis !== "string"){
validate135.errors = [{instancePath:instancePath+"/basis",schemaPath:"#/properties/basis/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.ranking !== undefined){
let data1 = data.ranking;
const _errs3 = errors;
if(errors === _errs3){
if(Array.isArray(data1)){
var valid1 = true;
const len0 = data1.length;
for(let i0=0; i0<len0; i0++){
const _errs5 = errors;
if(!(validate72(data1[i0], {instancePath:instancePath+"/ranking/" + i0,parentData:data1,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate72.errors : vErrors.concat(validate72.errors);
errors = vErrors.length;
}
var valid1 = _errs5 === errors;
if(!valid1){
break;
}
}
}
else {
validate135.errors = [{instancePath:instancePath+"/ranking",schemaPath:"#/properties/ranking/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
}
}
}
else {
validate135.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate135.errors = vErrors;
return errors === 0;
}
validate135.evaluated = {"props":{"basis":true,"ranking":true},"dynamicProps":false,"dynamicItems":false};

exports.concept_request = validate137;
const schema33 = {"additionalProperties":false,"properties":{"account_id":{"type":"string"},"candidate_count":{"default":5,"format":"uint16","maximum":10,"minimum":5,"type":"integer"},"concept":{"maxLength":64000,"minLength":1,"type":"string"},"constraints":{"default":"","maxLength":16000,"type":"string"},"criteria":{"default":[{"name":"Usefulness","weight":30},{"name":"Feasibility","weight":25},{"name":"Novelty","weight":20},{"name":"Supporting evidence","weight":15},{"name":"Risk management","weight":10}],"items":{"$ref":"#/$defs/Criterion"},"maxItems":10,"minItems":1,"type":"array"},"idempotency_key":{"type":"string"}},"required":["concept","account_id","idempotency_key"],"type":"object"};
const func1 = require("ajv/dist/runtime/ucs2length").default;
const schema34 = {"additionalProperties":false,"properties":{"name":{"maxLength":120,"minLength":1,"type":"string"},"weight":{"format":"uint16","maximum":100,"minimum":1,"type":"integer"}},"required":["name","weight"],"type":"object"};

function validate80(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate80.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.name === undefined) && (missing0 = "name")) || ((data.weight === undefined) && (missing0 = "weight"))){
validate80.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((key0 === "name") || (key0 === "weight"))){
validate80.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.name !== undefined){
let data0 = data.name;
const _errs2 = errors;
if(errors === _errs2){
if(typeof data0 === "string"){
if(func1(data0) > 120){
validate80.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/maxLength",keyword:"maxLength",params:{limit: 120},message:"must NOT have more than 120 characters"}];
return false;
}
else {
if(func1(data0) < 1){
validate80.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate80.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.weight !== undefined){
let data1 = data.weight;
const _errs4 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate80.errors = [{instancePath:instancePath+"/weight",schemaPath:"#/properties/weight/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs4){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 > 100 || isNaN(data1)){
validate80.errors = [{instancePath:instancePath+"/weight",schemaPath:"#/properties/weight/maximum",keyword:"maximum",params:{comparison: "<=", limit: 100},message:"must be <= 100"}];
return false;
}
else {
if(data1 < 1 || isNaN(data1)){
validate80.errors = [{instancePath:instancePath+"/weight",schemaPath:"#/properties/weight/minimum",keyword:"minimum",params:{comparison: ">=", limit: 1},message:"must be >= 1"}];
return false;
}
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate80.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate80.errors = vErrors;
return errors === 0;
}
validate80.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate137(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate137.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.concept === undefined) && (missing0 = "concept")) || ((data.account_id === undefined) && (missing0 = "account_id"))) || ((data.idempotency_key === undefined) && (missing0 = "idempotency_key"))){
validate137.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((((key0 === "account_id") || (key0 === "candidate_count")) || (key0 === "concept")) || (key0 === "constraints")) || (key0 === "criteria")) || (key0 === "idempotency_key"))){
validate137.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.account_id !== undefined){
const _errs2 = errors;
if(typeof data.account_id !== "string"){
validate137.errors = [{instancePath:instancePath+"/account_id",schemaPath:"#/properties/account_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.candidate_count !== undefined){
let data1 = data.candidate_count;
const _errs4 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate137.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs4){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 > 10 || isNaN(data1)){
validate137.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 10},message:"must be <= 10"}];
return false;
}
else {
if(data1 < 5 || isNaN(data1)){
validate137.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 5},message:"must be >= 5"}];
return false;
}
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.concept !== undefined){
let data2 = data.concept;
const _errs6 = errors;
if(errors === _errs6){
if(typeof data2 === "string"){
if(func1(data2) > 64000){
validate137.errors = [{instancePath:instancePath+"/concept",schemaPath:"#/properties/concept/maxLength",keyword:"maxLength",params:{limit: 64000},message:"must NOT have more than 64000 characters"}];
return false;
}
else {
if(func1(data2) < 1){
validate137.errors = [{instancePath:instancePath+"/concept",schemaPath:"#/properties/concept/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate137.errors = [{instancePath:instancePath+"/concept",schemaPath:"#/properties/concept/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.constraints !== undefined){
let data3 = data.constraints;
const _errs8 = errors;
if(errors === _errs8){
if(typeof data3 === "string"){
if(func1(data3) > 16000){
validate137.errors = [{instancePath:instancePath+"/constraints",schemaPath:"#/properties/constraints/maxLength",keyword:"maxLength",params:{limit: 16000},message:"must NOT have more than 16000 characters"}];
return false;
}
}
else {
validate137.errors = [{instancePath:instancePath+"/constraints",schemaPath:"#/properties/constraints/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.criteria !== undefined){
let data4 = data.criteria;
const _errs10 = errors;
if(errors === _errs10){
if(Array.isArray(data4)){
if(data4.length > 10){
validate137.errors = [{instancePath:instancePath+"/criteria",schemaPath:"#/properties/criteria/maxItems",keyword:"maxItems",params:{limit: 10},message:"must NOT have more than 10 items"}];
return false;
}
else {
if(data4.length < 1){
validate137.errors = [{instancePath:instancePath+"/criteria",schemaPath:"#/properties/criteria/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid1 = true;
const len0 = data4.length;
for(let i0=0; i0<len0; i0++){
const _errs12 = errors;
if(!(validate80(data4[i0], {instancePath:instancePath+"/criteria/" + i0,parentData:data4,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate80.errors : vErrors.concat(validate80.errors);
errors = vErrors.length;
}
var valid1 = _errs12 === errors;
if(!valid1){
break;
}
}
}
}
}
else {
validate137.errors = [{instancePath:instancePath+"/criteria",schemaPath:"#/properties/criteria/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs10 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.idempotency_key !== undefined){
const _errs13 = errors;
if(typeof data.idempotency_key !== "string"){
validate137.errors = [{instancePath:instancePath+"/idempotency_key",schemaPath:"#/properties/idempotency_key/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
else {
validate137.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate137.errors = vErrors;
return errors === 0;
}
validate137.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.confirm_account = validate139;
const schema35 = {"additionalProperties":false,"properties":{"identity":{"$ref":"#/$defs/AccountIdentity"},"model":{"$ref":"#/$defs/ModelSelection"}},"required":["identity","model"],"type":"object"};

function validate139(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate139.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.identity === undefined) && (missing0 = "identity")) || ((data.model === undefined) && (missing0 = "model"))){
validate139.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((key0 === "identity") || (key0 === "model"))){
validate139.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.identity !== undefined){
const _errs2 = errors;
if(!(validate59(data.identity, {instancePath:instancePath+"/identity",parentData:data,parentDataProperty:"identity",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate59.errors : vErrors.concat(validate59.errors);
errors = vErrors.length;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.model !== undefined){
const _errs3 = errors;
if(!(validate56(data.model, {instancePath:instancePath+"/model",parentData:data,parentDataProperty:"model",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate56.errors : vErrors.concat(validate56.errors);
errors = vErrors.length;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate139.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate139.errors = vErrors;
return errors === 0;
}
validate139.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.connect_account = validate142;
const schema36 = {"additionalProperties":false,"properties":{"email":{"type":"string"},"id":{"type":["string","null"]}},"required":["email"],"type":"object"};

function validate142(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate142.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((data.email === undefined) && (missing0 = "email")){
validate142.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((key0 === "email") || (key0 === "id"))){
validate142.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.email !== undefined){
const _errs2 = errors;
if(typeof data.email !== "string"){
validate142.errors = [{instancePath:instancePath+"/email",schemaPath:"#/properties/email/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
let data1 = data.id;
const _errs4 = errors;
if((typeof data1 !== "string") && (data1 !== null)){
validate142.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: schema36.properties.id.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate142.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate142.errors = vErrors;
return errors === 0;
}
validate142.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.created_token = validate143;
const schema37 = {"properties":{"metadata":{"$ref":"#/$defs/TokenMetadata"},"secret":{"description":"Returned once. Only its SHA-256 digest is retained by the installation.","type":"string"}},"required":["metadata","secret"],"type":"object"};
const schema38 = {"properties":{"account_ids":{"items":{"type":"string"},"type":"array"},"created_ms":{"format":"int64","type":"integer"},"id":{"type":"string"},"name":{"type":"string"},"revoked_ms":{"format":"int64","type":["integer","null"]}},"required":["id","name","account_ids","created_ms"],"type":"object"};

function validate90(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate90.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.id === undefined) && (missing0 = "id")) || ((data.name === undefined) && (missing0 = "name"))) || ((data.account_ids === undefined) && (missing0 = "account_ids"))) || ((data.created_ms === undefined) && (missing0 = "created_ms"))){
validate90.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.account_ids !== undefined){
let data0 = data.account_ids;
const _errs1 = errors;
if(errors === _errs1){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs3 = errors;
if(typeof data0[i0] !== "string"){
validate90.errors = [{instancePath:instancePath+"/account_ids/" + i0,schemaPath:"#/properties/account_ids/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs3 === errors;
if(!valid1){
break;
}
}
}
else {
validate90.errors = [{instancePath:instancePath+"/account_ids",schemaPath:"#/properties/account_ids/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_ms !== undefined){
let data2 = data.created_ms;
const _errs5 = errors;
if(!(((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2))) && (isFinite(data2)))){
validate90.errors = [{instancePath:instancePath+"/created_ms",schemaPath:"#/properties/created_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs7 = errors;
if(typeof data.id !== "string"){
validate90.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.name !== undefined){
const _errs9 = errors;
if(typeof data.name !== "string"){
validate90.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revoked_ms !== undefined){
let data5 = data.revoked_ms;
const _errs11 = errors;
if((!(((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5))) && (isFinite(data5)))) && (data5 !== null)){
validate90.errors = [{instancePath:instancePath+"/revoked_ms",schemaPath:"#/properties/revoked_ms/type",keyword:"type",params:{type: schema38.properties.revoked_ms.type},message:"must be integer,null"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate90.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate90.errors = vErrors;
return errors === 0;
}
validate90.evaluated = {"props":{"account_ids":true,"created_ms":true,"id":true,"name":true,"revoked_ms":true},"dynamicProps":false,"dynamicItems":false};


function validate143(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate143.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.metadata === undefined) && (missing0 = "metadata")) || ((data.secret === undefined) && (missing0 = "secret"))){
validate143.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.metadata !== undefined){
const _errs1 = errors;
if(!(validate90(data.metadata, {instancePath:instancePath+"/metadata",parentData:data,parentDataProperty:"metadata",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate90.errors : vErrors.concat(validate90.errors);
errors = vErrors.length;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.secret !== undefined){
const _errs2 = errors;
if(typeof data.secret !== "string"){
validate143.errors = [{instancePath:instancePath+"/secret",schemaPath:"#/properties/secret/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
}
}
}
else {
validate143.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate143.errors = vErrors;
return errors === 0;
}
validate143.evaluated = {"props":{"metadata":true,"secret":true},"dynamicProps":false,"dynamicItems":false};

exports.error = validate145;
const schema39 = {"properties":{"code":{"type":"string"},"message":{"type":"string"},"next_action":{"type":"string"}},"required":["code","message","next_action"],"type":"object"};

function validate145(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate145.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.code === undefined) && (missing0 = "code")) || ((data.message === undefined) && (missing0 = "message"))) || ((data.next_action === undefined) && (missing0 = "next_action"))){
validate145.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.code !== undefined){
const _errs1 = errors;
if(typeof data.code !== "string"){
validate145.errors = [{instancePath:instancePath+"/code",schemaPath:"#/properties/code/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.message !== undefined){
const _errs3 = errors;
if(typeof data.message !== "string"){
validate145.errors = [{instancePath:instancePath+"/message",schemaPath:"#/properties/message/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_action !== undefined){
const _errs5 = errors;
if(typeof data.next_action !== "string"){
validate145.errors = [{instancePath:instancePath+"/next_action",schemaPath:"#/properties/next_action/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate145.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate145.errors = vErrors;
return errors === 0;
}
validate145.evaluated = {"props":{"code":true,"message":true,"next_action":true},"dynamicProps":false,"dynamicItems":false};

exports.event = validate146;
const schema40 = {"properties":{"created_ms":{"format":"int64","type":"integer"},"data":true,"kind":{"type":"string"},"run_id":{"type":"string"},"sequence":{"format":"int64","type":"integer"}},"required":["sequence","run_id","kind","data","created_ms"],"type":"object"};

function validate146(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate146.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((data.sequence === undefined) && (missing0 = "sequence")) || ((data.run_id === undefined) && (missing0 = "run_id"))) || ((data.kind === undefined) && (missing0 = "kind"))) || ((data.data === undefined) && (missing0 = "data"))) || ((data.created_ms === undefined) && (missing0 = "created_ms"))){
validate146.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.created_ms !== undefined){
let data0 = data.created_ms;
const _errs1 = errors;
if(!(((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0))) && (isFinite(data0)))){
validate146.errors = [{instancePath:instancePath+"/created_ms",schemaPath:"#/properties/created_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.kind !== undefined){
const _errs3 = errors;
if(typeof data.kind !== "string"){
validate146.errors = [{instancePath:instancePath+"/kind",schemaPath:"#/properties/kind/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
const _errs5 = errors;
if(typeof data.run_id !== "string"){
validate146.errors = [{instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.sequence !== undefined){
let data3 = data.sequence;
const _errs7 = errors;
if(!(((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3))) && (isFinite(data3)))){
validate146.errors = [{instancePath:instancePath+"/sequence",schemaPath:"#/properties/sequence/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate146.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate146.errors = vErrors;
return errors === 0;
}
validate146.evaluated = {"props":{"created_ms":true,"data":true,"kind":true,"run_id":true,"sequence":true},"dynamicProps":false,"dynamicItems":false};

exports.final_result = validate147;
const schema41 = {"properties":{"artifacts":{"items":{"$ref":"#/$defs/Artifact"},"type":"array"},"configuration":{"$ref":"#/$defs/ConfigurationSnapshot"},"evaluation_notice":{"type":"string"},"excluded_candidates":{"items":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"type":"array"},"final_artifact":{"anyOf":[{"$ref":"#/$defs/Artifact"},{"type":"null"}],"default":null,"description":"Accepted final text, populated when reading historical manifests too."},"request":{"$ref":"#/$defs/ConceptRequest"},"run_id":{"type":"string"},"schema_version":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"status":{"type":"string"}},"required":["schema_version","run_id","status","request","configuration","excluded_candidates","evaluation_notice","artifacts"],"type":"object"};
const schema42 = {"properties":{"max_attempts":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"max_jitter_ms":{"format":"int64","type":"integer"},"model":{"$ref":"#/$defs/ModelSelection"},"overall_deadline_ms":{"format":"int64","type":"integer"},"perspectives":{"items":{"type":"string"},"type":"array"},"response_timeout_ms":{"format":"int64","type":"integer"},"stage_char_limit":{"format":"uint","minimum":0,"type":"integer"},"submission_spacing_ms":{"format":"int64","type":"integer"},"summary_char_limit":{"format":"uint","minimum":0,"type":"integer"},"template_version":{"type":"string"}},"required":["template_version","perspectives","model","submission_spacing_ms","max_jitter_ms","response_timeout_ms","overall_deadline_ms","max_attempts","summary_char_limit","stage_char_limit"],"type":"object"};

function validate99(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate99.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((((((((data.template_version === undefined) && (missing0 = "template_version")) || ((data.perspectives === undefined) && (missing0 = "perspectives"))) || ((data.model === undefined) && (missing0 = "model"))) || ((data.submission_spacing_ms === undefined) && (missing0 = "submission_spacing_ms"))) || ((data.max_jitter_ms === undefined) && (missing0 = "max_jitter_ms"))) || ((data.response_timeout_ms === undefined) && (missing0 = "response_timeout_ms"))) || ((data.overall_deadline_ms === undefined) && (missing0 = "overall_deadline_ms"))) || ((data.max_attempts === undefined) && (missing0 = "max_attempts"))) || ((data.summary_char_limit === undefined) && (missing0 = "summary_char_limit"))) || ((data.stage_char_limit === undefined) && (missing0 = "stage_char_limit"))){
validate99.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.max_attempts !== undefined){
let data0 = data.max_attempts;
const _errs1 = errors;
if(!(((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0))) && (isFinite(data0)))){
validate99.errors = [{instancePath:instancePath+"/max_attempts",schemaPath:"#/properties/max_attempts/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs1){
if((typeof data0 == "number") && (isFinite(data0))){
if(data0 > 65535 || isNaN(data0)){
validate99.errors = [{instancePath:instancePath+"/max_attempts",schemaPath:"#/properties/max_attempts/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data0 < 0 || isNaN(data0)){
validate99.errors = [{instancePath:instancePath+"/max_attempts",schemaPath:"#/properties/max_attempts/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.max_jitter_ms !== undefined){
let data1 = data.max_jitter_ms;
const _errs3 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate99.errors = [{instancePath:instancePath+"/max_jitter_ms",schemaPath:"#/properties/max_jitter_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.model !== undefined){
const _errs5 = errors;
if(!(validate56(data.model, {instancePath:instancePath+"/model",parentData:data,parentDataProperty:"model",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate56.errors : vErrors.concat(validate56.errors);
errors = vErrors.length;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.overall_deadline_ms !== undefined){
let data3 = data.overall_deadline_ms;
const _errs6 = errors;
if(!(((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3))) && (isFinite(data3)))){
validate99.errors = [{instancePath:instancePath+"/overall_deadline_ms",schemaPath:"#/properties/overall_deadline_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.perspectives !== undefined){
let data4 = data.perspectives;
const _errs8 = errors;
if(errors === _errs8){
if(Array.isArray(data4)){
var valid1 = true;
const len0 = data4.length;
for(let i0=0; i0<len0; i0++){
const _errs10 = errors;
if(typeof data4[i0] !== "string"){
validate99.errors = [{instancePath:instancePath+"/perspectives/" + i0,schemaPath:"#/properties/perspectives/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs10 === errors;
if(!valid1){
break;
}
}
}
else {
validate99.errors = [{instancePath:instancePath+"/perspectives",schemaPath:"#/properties/perspectives/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.response_timeout_ms !== undefined){
let data6 = data.response_timeout_ms;
const _errs12 = errors;
if(!(((typeof data6 == "number") && (!(data6 % 1) && !isNaN(data6))) && (isFinite(data6)))){
validate99.errors = [{instancePath:instancePath+"/response_timeout_ms",schemaPath:"#/properties/response_timeout_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.stage_char_limit !== undefined){
let data7 = data.stage_char_limit;
const _errs14 = errors;
if(!(((typeof data7 == "number") && (!(data7 % 1) && !isNaN(data7))) && (isFinite(data7)))){
validate99.errors = [{instancePath:instancePath+"/stage_char_limit",schemaPath:"#/properties/stage_char_limit/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs14){
if((typeof data7 == "number") && (isFinite(data7))){
if(data7 < 0 || isNaN(data7)){
validate99.errors = [{instancePath:instancePath+"/stage_char_limit",schemaPath:"#/properties/stage_char_limit/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs14 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.submission_spacing_ms !== undefined){
let data8 = data.submission_spacing_ms;
const _errs16 = errors;
if(!(((typeof data8 == "number") && (!(data8 % 1) && !isNaN(data8))) && (isFinite(data8)))){
validate99.errors = [{instancePath:instancePath+"/submission_spacing_ms",schemaPath:"#/properties/submission_spacing_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.summary_char_limit !== undefined){
let data9 = data.summary_char_limit;
const _errs18 = errors;
if(!(((typeof data9 == "number") && (!(data9 % 1) && !isNaN(data9))) && (isFinite(data9)))){
validate99.errors = [{instancePath:instancePath+"/summary_char_limit",schemaPath:"#/properties/summary_char_limit/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs18){
if((typeof data9 == "number") && (isFinite(data9))){
if(data9 < 0 || isNaN(data9)){
validate99.errors = [{instancePath:instancePath+"/summary_char_limit",schemaPath:"#/properties/summary_char_limit/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs18 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.template_version !== undefined){
const _errs20 = errors;
if(typeof data.template_version !== "string"){
validate99.errors = [{instancePath:instancePath+"/template_version",schemaPath:"#/properties/template_version/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs20 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate99.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate99.errors = vErrors;
return errors === 0;
}
validate99.evaluated = {"props":{"max_attempts":true,"max_jitter_ms":true,"model":true,"overall_deadline_ms":true,"perspectives":true,"response_timeout_ms":true,"stage_char_limit":true,"submission_spacing_ms":true,"summary_char_limit":true,"template_version":true},"dynamicProps":false,"dynamicItems":false};


function validate79(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate79.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.concept === undefined) && (missing0 = "concept")) || ((data.account_id === undefined) && (missing0 = "account_id"))) || ((data.idempotency_key === undefined) && (missing0 = "idempotency_key"))){
validate79.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((((((key0 === "account_id") || (key0 === "candidate_count")) || (key0 === "concept")) || (key0 === "constraints")) || (key0 === "criteria")) || (key0 === "idempotency_key"))){
validate79.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.account_id !== undefined){
const _errs2 = errors;
if(typeof data.account_id !== "string"){
validate79.errors = [{instancePath:instancePath+"/account_id",schemaPath:"#/properties/account_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.candidate_count !== undefined){
let data1 = data.candidate_count;
const _errs4 = errors;
if(!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))){
validate79.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs4){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 > 10 || isNaN(data1)){
validate79.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/maximum",keyword:"maximum",params:{comparison: "<=", limit: 10},message:"must be <= 10"}];
return false;
}
else {
if(data1 < 5 || isNaN(data1)){
validate79.errors = [{instancePath:instancePath+"/candidate_count",schemaPath:"#/properties/candidate_count/minimum",keyword:"minimum",params:{comparison: ">=", limit: 5},message:"must be >= 5"}];
return false;
}
}
}
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.concept !== undefined){
let data2 = data.concept;
const _errs6 = errors;
if(errors === _errs6){
if(typeof data2 === "string"){
if(func1(data2) > 64000){
validate79.errors = [{instancePath:instancePath+"/concept",schemaPath:"#/properties/concept/maxLength",keyword:"maxLength",params:{limit: 64000},message:"must NOT have more than 64000 characters"}];
return false;
}
else {
if(func1(data2) < 1){
validate79.errors = [{instancePath:instancePath+"/concept",schemaPath:"#/properties/concept/minLength",keyword:"minLength",params:{limit: 1},message:"must NOT have fewer than 1 characters"}];
return false;
}
}
}
else {
validate79.errors = [{instancePath:instancePath+"/concept",schemaPath:"#/properties/concept/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.constraints !== undefined){
let data3 = data.constraints;
const _errs8 = errors;
if(errors === _errs8){
if(typeof data3 === "string"){
if(func1(data3) > 16000){
validate79.errors = [{instancePath:instancePath+"/constraints",schemaPath:"#/properties/constraints/maxLength",keyword:"maxLength",params:{limit: 16000},message:"must NOT have more than 16000 characters"}];
return false;
}
}
else {
validate79.errors = [{instancePath:instancePath+"/constraints",schemaPath:"#/properties/constraints/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.criteria !== undefined){
let data4 = data.criteria;
const _errs10 = errors;
if(errors === _errs10){
if(Array.isArray(data4)){
if(data4.length > 10){
validate79.errors = [{instancePath:instancePath+"/criteria",schemaPath:"#/properties/criteria/maxItems",keyword:"maxItems",params:{limit: 10},message:"must NOT have more than 10 items"}];
return false;
}
else {
if(data4.length < 1){
validate79.errors = [{instancePath:instancePath+"/criteria",schemaPath:"#/properties/criteria/minItems",keyword:"minItems",params:{limit: 1},message:"must NOT have fewer than 1 items"}];
return false;
}
else {
var valid1 = true;
const len0 = data4.length;
for(let i0=0; i0<len0; i0++){
const _errs12 = errors;
if(!(validate80(data4[i0], {instancePath:instancePath+"/criteria/" + i0,parentData:data4,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate80.errors : vErrors.concat(validate80.errors);
errors = vErrors.length;
}
var valid1 = _errs12 === errors;
if(!valid1){
break;
}
}
}
}
}
else {
validate79.errors = [{instancePath:instancePath+"/criteria",schemaPath:"#/properties/criteria/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs10 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.idempotency_key !== undefined){
const _errs13 = errors;
if(typeof data.idempotency_key !== "string"){
validate79.errors = [{instancePath:instancePath+"/idempotency_key",schemaPath:"#/properties/idempotency_key/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
else {
validate79.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate79.errors = vErrors;
return errors === 0;
}
validate79.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate147(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate147.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((((((data.schema_version === undefined) && (missing0 = "schema_version")) || ((data.run_id === undefined) && (missing0 = "run_id"))) || ((data.status === undefined) && (missing0 = "status"))) || ((data.request === undefined) && (missing0 = "request"))) || ((data.configuration === undefined) && (missing0 = "configuration"))) || ((data.excluded_candidates === undefined) && (missing0 = "excluded_candidates"))) || ((data.evaluation_notice === undefined) && (missing0 = "evaluation_notice"))) || ((data.artifacts === undefined) && (missing0 = "artifacts"))){
validate147.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.artifacts !== undefined){
let data0 = data.artifacts;
const _errs1 = errors;
if(errors === _errs1){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs3 = errors;
if(!(validate64(data0[i0], {instancePath:instancePath+"/artifacts/" + i0,parentData:data0,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate64.errors : vErrors.concat(validate64.errors);
errors = vErrors.length;
}
var valid1 = _errs3 === errors;
if(!valid1){
break;
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/artifacts",schemaPath:"#/properties/artifacts/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.configuration !== undefined){
const _errs4 = errors;
if(!(validate99(data.configuration, {instancePath:instancePath+"/configuration",parentData:data,parentDataProperty:"configuration",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate99.errors : vErrors.concat(validate99.errors);
errors = vErrors.length;
}
var valid0 = _errs4 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.evaluation_notice !== undefined){
const _errs5 = errors;
if(typeof data.evaluation_notice !== "string"){
validate147.errors = [{instancePath:instancePath+"/evaluation_notice",schemaPath:"#/properties/evaluation_notice/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.excluded_candidates !== undefined){
let data4 = data.excluded_candidates;
const _errs7 = errors;
if(errors === _errs7){
if(Array.isArray(data4)){
var valid2 = true;
const len1 = data4.length;
for(let i1=0; i1<len1; i1++){
let data5 = data4[i1];
const _errs9 = errors;
if(!(((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5))) && (isFinite(data5)))){
validate147.errors = [{instancePath:instancePath+"/excluded_candidates/" + i1,schemaPath:"#/properties/excluded_candidates/items/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs9){
if((typeof data5 == "number") && (isFinite(data5))){
if(data5 > 65535 || isNaN(data5)){
validate147.errors = [{instancePath:instancePath+"/excluded_candidates/" + i1,schemaPath:"#/properties/excluded_candidates/items/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data5 < 0 || isNaN(data5)){
validate147.errors = [{instancePath:instancePath+"/excluded_candidates/" + i1,schemaPath:"#/properties/excluded_candidates/items/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid2 = _errs9 === errors;
if(!valid2){
break;
}
}
}
else {
validate147.errors = [{instancePath:instancePath+"/excluded_candidates",schemaPath:"#/properties/excluded_candidates/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.final_artifact !== undefined){
let data6 = data.final_artifact;
const _errs11 = errors;
const _errs12 = errors;
let valid3 = false;
const _errs13 = errors;
if(!(validate64(data6, {instancePath:instancePath+"/final_artifact",parentData:data,parentDataProperty:"final_artifact",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate64.errors : vErrors.concat(validate64.errors);
errors = vErrors.length;
}
var _valid0 = _errs13 === errors;
valid3 = valid3 || _valid0;
if(_valid0){
var props0 = {};
props0.attempt_id = true;
props0.byte_length = true;
props0.completion = true;
props0.created_ms = true;
props0.id = true;
props0.media_type = true;
props0.name = true;
props0.run_id = true;
props0.sha256 = true;
}
const _errs14 = errors;
if(data6 !== null){
const err0 = {instancePath:instancePath+"/final_artifact",schemaPath:"#/properties/final_artifact/anyOf/1/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs14 === errors;
valid3 = valid3 || _valid0;
if(!valid3){
const err1 = {instancePath:instancePath+"/final_artifact",schemaPath:"#/properties/final_artifact/anyOf",keyword:"anyOf",params:{},message:"must match a schema in anyOf"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
validate147.errors = vErrors;
return false;
}
else {
errors = _errs12;
if(vErrors !== null){
if(_errs12){
vErrors.length = _errs12;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.request !== undefined){
const _errs16 = errors;
if(!(validate79(data.request, {instancePath:instancePath+"/request",parentData:data,parentDataProperty:"request",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate79.errors : vErrors.concat(validate79.errors);
errors = vErrors.length;
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
const _errs17 = errors;
if(typeof data.run_id !== "string"){
validate147.errors = [{instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs17 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.schema_version !== undefined){
let data9 = data.schema_version;
const _errs19 = errors;
if(!(((typeof data9 == "number") && (!(data9 % 1) && !isNaN(data9))) && (isFinite(data9)))){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/properties/schema_version/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs19){
if((typeof data9 == "number") && (isFinite(data9))){
if(data9 > 65535 || isNaN(data9)){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/properties/schema_version/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data9 < 0 || isNaN(data9)){
validate147.errors = [{instancePath:instancePath+"/schema_version",schemaPath:"#/properties/schema_version/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs19 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.status !== undefined){
const _errs21 = errors;
if(typeof data.status !== "string"){
validate147.errors = [{instancePath:instancePath+"/status",schemaPath:"#/properties/status/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs21 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
else {
validate147.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate147.errors = vErrors;
return errors === 0;
}
validate147.evaluated = {"props":{"artifacts":true,"configuration":true,"evaluation_notice":true,"excluded_candidates":true,"final_artifact":true,"request":true,"run_id":true,"schema_version":true,"status":true},"dynamicProps":false,"dynamicItems":false};

exports.issue_token = validate152;
const schema43 = {"additionalProperties":false,"properties":{"account_ids":{"description":"Explicit account IDs; future accounts are never added implicitly.","items":{"type":"string"},"type":"array"},"name":{"type":"string"}},"required":["name","account_ids"],"type":"object"};

function validate152(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate152.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((data.name === undefined) && (missing0 = "name")) || ((data.account_ids === undefined) && (missing0 = "account_ids"))){
validate152.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!((key0 === "account_ids") || (key0 === "name"))){
validate152.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.account_ids !== undefined){
let data0 = data.account_ids;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs4 = errors;
if(typeof data0[i0] !== "string"){
validate152.errors = [{instancePath:instancePath+"/account_ids/" + i0,schemaPath:"#/properties/account_ids/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate152.errors = [{instancePath:instancePath+"/account_ids",schemaPath:"#/properties/account_ids/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.name !== undefined){
const _errs6 = errors;
if(typeof data.name !== "string"){
validate152.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
else {
validate152.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate152.errors = vErrors;
return errors === 0;
}
validate152.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.resume_request = validate153;
const schema44 = {"additionalProperties":false,"properties":{"allow_incomplete":{"default":false,"type":"boolean"}},"type":"object"};

function validate153(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate153.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
const _errs1 = errors;
for(const key0 in data){
if(!(key0 === "allow_incomplete")){
validate153.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.allow_incomplete !== undefined){
if(typeof data.allow_incomplete !== "boolean"){
validate153.errors = [{instancePath:instancePath+"/allow_incomplete",schemaPath:"#/properties/allow_incomplete/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
}
}
}
else {
validate153.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate153.errors = vErrors;
return errors === 0;
}
validate153.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.run = validate154;
const schema45 = {"properties":{"allow_incomplete":{"type":"boolean"},"artifacts":{"items":{"$ref":"#/$defs/Artifact"},"type":"array"},"configuration":{"$ref":"#/$defs/ConfigurationSnapshot"},"created_ms":{"format":"int64","type":"integer"},"deadline_ms":{"format":"int64","type":"integer"},"id":{"type":"string"},"pause_reason":{"type":["string","null"]},"request":{"$ref":"#/$defs/ConceptRequest"},"status":{"type":"string"},"submission_limit":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"submissions":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"tasks":{"items":{"$ref":"#/$defs/Task"},"type":"array"}},"required":["id","request","configuration","status","created_ms","deadline_ms","submissions","submission_limit","allow_incomplete","tasks","artifacts"],"type":"object"};
const schema46 = {"properties":{"attempts":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"error_code":{"type":["string","null"]},"id":{"type":"string"},"position":{"format":"uint16","maximum":65535,"minimum":0,"type":"integer"},"stage":{"type":"string"},"status":{"type":"string"},"summary":{"anyOf":[{"$ref":"#/$defs/CandidateSummary"},{"type":"null"}]}},"required":["id","stage","position","status","attempts"],"type":"object"};
const schema47 = {"additionalProperties":false,"properties":{"assumptions":{"items":{"type":"string"},"type":"array"},"next_steps":{"items":{"type":"string"},"type":"array"},"proposal":{"type":"string"},"strengths":{"items":{"type":"string"},"type":"array"},"weaknesses":{"items":{"type":"string"},"type":"array"}},"required":["proposal","strengths","weaknesses","assumptions","next_steps"],"type":"object"};

function validate114(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate114.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((data.proposal === undefined) && (missing0 = "proposal")) || ((data.strengths === undefined) && (missing0 = "strengths"))) || ((data.weaknesses === undefined) && (missing0 = "weaknesses"))) || ((data.assumptions === undefined) && (missing0 = "assumptions"))) || ((data.next_steps === undefined) && (missing0 = "next_steps"))){
validate114.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(((((key0 === "assumptions") || (key0 === "next_steps")) || (key0 === "proposal")) || (key0 === "strengths")) || (key0 === "weaknesses"))){
validate114.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.assumptions !== undefined){
let data0 = data.assumptions;
const _errs2 = errors;
if(errors === _errs2){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs4 = errors;
if(typeof data0[i0] !== "string"){
validate114.errors = [{instancePath:instancePath+"/assumptions/" + i0,schemaPath:"#/properties/assumptions/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs4 === errors;
if(!valid1){
break;
}
}
}
else {
validate114.errors = [{instancePath:instancePath+"/assumptions",schemaPath:"#/properties/assumptions/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs2 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.next_steps !== undefined){
let data2 = data.next_steps;
const _errs6 = errors;
if(errors === _errs6){
if(Array.isArray(data2)){
var valid2 = true;
const len1 = data2.length;
for(let i1=0; i1<len1; i1++){
const _errs8 = errors;
if(typeof data2[i1] !== "string"){
validate114.errors = [{instancePath:instancePath+"/next_steps/" + i1,schemaPath:"#/properties/next_steps/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid2 = _errs8 === errors;
if(!valid2){
break;
}
}
}
else {
validate114.errors = [{instancePath:instancePath+"/next_steps",schemaPath:"#/properties/next_steps/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.proposal !== undefined){
const _errs10 = errors;
if(typeof data.proposal !== "string"){
validate114.errors = [{instancePath:instancePath+"/proposal",schemaPath:"#/properties/proposal/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs10 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.strengths !== undefined){
let data5 = data.strengths;
const _errs12 = errors;
if(errors === _errs12){
if(Array.isArray(data5)){
var valid3 = true;
const len2 = data5.length;
for(let i2=0; i2<len2; i2++){
const _errs14 = errors;
if(typeof data5[i2] !== "string"){
validate114.errors = [{instancePath:instancePath+"/strengths/" + i2,schemaPath:"#/properties/strengths/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid3 = _errs14 === errors;
if(!valid3){
break;
}
}
}
else {
validate114.errors = [{instancePath:instancePath+"/strengths",schemaPath:"#/properties/strengths/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs12 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.weaknesses !== undefined){
let data7 = data.weaknesses;
const _errs16 = errors;
if(errors === _errs16){
if(Array.isArray(data7)){
var valid4 = true;
const len3 = data7.length;
for(let i3=0; i3<len3; i3++){
const _errs18 = errors;
if(typeof data7[i3] !== "string"){
validate114.errors = [{instancePath:instancePath+"/weaknesses/" + i3,schemaPath:"#/properties/weaknesses/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid4 = _errs18 === errors;
if(!valid4){
break;
}
}
}
else {
validate114.errors = [{instancePath:instancePath+"/weaknesses",schemaPath:"#/properties/weaknesses/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
else {
validate114.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate114.errors = vErrors;
return errors === 0;
}
validate114.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};


function validate113(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate113.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((data.id === undefined) && (missing0 = "id")) || ((data.stage === undefined) && (missing0 = "stage"))) || ((data.position === undefined) && (missing0 = "position"))) || ((data.status === undefined) && (missing0 = "status"))) || ((data.attempts === undefined) && (missing0 = "attempts"))){
validate113.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.attempts !== undefined){
let data0 = data.attempts;
const _errs1 = errors;
if(!(((typeof data0 == "number") && (!(data0 % 1) && !isNaN(data0))) && (isFinite(data0)))){
validate113.errors = [{instancePath:instancePath+"/attempts",schemaPath:"#/properties/attempts/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs1){
if((typeof data0 == "number") && (isFinite(data0))){
if(data0 > 65535 || isNaN(data0)){
validate113.errors = [{instancePath:instancePath+"/attempts",schemaPath:"#/properties/attempts/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data0 < 0 || isNaN(data0)){
validate113.errors = [{instancePath:instancePath+"/attempts",schemaPath:"#/properties/attempts/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.error_code !== undefined){
let data1 = data.error_code;
const _errs3 = errors;
if((typeof data1 !== "string") && (data1 !== null)){
validate113.errors = [{instancePath:instancePath+"/error_code",schemaPath:"#/properties/error_code/type",keyword:"type",params:{type: schema46.properties.error_code.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs5 = errors;
if(typeof data.id !== "string"){
validate113.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.position !== undefined){
let data3 = data.position;
const _errs7 = errors;
if(!(((typeof data3 == "number") && (!(data3 % 1) && !isNaN(data3))) && (isFinite(data3)))){
validate113.errors = [{instancePath:instancePath+"/position",schemaPath:"#/properties/position/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs7){
if((typeof data3 == "number") && (isFinite(data3))){
if(data3 > 65535 || isNaN(data3)){
validate113.errors = [{instancePath:instancePath+"/position",schemaPath:"#/properties/position/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data3 < 0 || isNaN(data3)){
validate113.errors = [{instancePath:instancePath+"/position",schemaPath:"#/properties/position/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.stage !== undefined){
const _errs9 = errors;
if(typeof data.stage !== "string"){
validate113.errors = [{instancePath:instancePath+"/stage",schemaPath:"#/properties/stage/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.status !== undefined){
const _errs11 = errors;
if(typeof data.status !== "string"){
validate113.errors = [{instancePath:instancePath+"/status",schemaPath:"#/properties/status/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.summary !== undefined){
let data6 = data.summary;
const _errs13 = errors;
const _errs14 = errors;
let valid1 = false;
const _errs15 = errors;
if(!(validate114(data6, {instancePath:instancePath+"/summary",parentData:data,parentDataProperty:"summary",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate114.errors : vErrors.concat(validate114.errors);
errors = vErrors.length;
}
var _valid0 = _errs15 === errors;
valid1 = valid1 || _valid0;
const _errs16 = errors;
if(data6 !== null){
const err0 = {instancePath:instancePath+"/summary",schemaPath:"#/properties/summary/anyOf/1/type",keyword:"type",params:{type: "null"},message:"must be null"};
if(vErrors === null){
vErrors = [err0];
}
else {
vErrors.push(err0);
}
errors++;
}
var _valid0 = _errs16 === errors;
valid1 = valid1 || _valid0;
if(!valid1){
const err1 = {instancePath:instancePath+"/summary",schemaPath:"#/properties/summary/anyOf",keyword:"anyOf",params:{},message:"must match a schema in anyOf"};
if(vErrors === null){
vErrors = [err1];
}
else {
vErrors.push(err1);
}
errors++;
validate113.errors = vErrors;
return false;
}
else {
errors = _errs14;
if(vErrors !== null){
if(_errs14){
vErrors.length = _errs14;
}
else {
vErrors = null;
}
}
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
else {
validate113.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate113.errors = vErrors;
return errors === 0;
}
validate113.evaluated = {"props":{"attempts":true,"error_code":true,"id":true,"position":true,"stage":true,"status":true,"summary":true},"dynamicProps":false,"dynamicItems":false};


function validate154(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate154.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((((((((((data.id === undefined) && (missing0 = "id")) || ((data.request === undefined) && (missing0 = "request"))) || ((data.configuration === undefined) && (missing0 = "configuration"))) || ((data.status === undefined) && (missing0 = "status"))) || ((data.created_ms === undefined) && (missing0 = "created_ms"))) || ((data.deadline_ms === undefined) && (missing0 = "deadline_ms"))) || ((data.submissions === undefined) && (missing0 = "submissions"))) || ((data.submission_limit === undefined) && (missing0 = "submission_limit"))) || ((data.allow_incomplete === undefined) && (missing0 = "allow_incomplete"))) || ((data.tasks === undefined) && (missing0 = "tasks"))) || ((data.artifacts === undefined) && (missing0 = "artifacts"))){
validate154.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.allow_incomplete !== undefined){
const _errs1 = errors;
if(typeof data.allow_incomplete !== "boolean"){
validate154.errors = [{instancePath:instancePath+"/allow_incomplete",schemaPath:"#/properties/allow_incomplete/type",keyword:"type",params:{type: "boolean"},message:"must be boolean"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.artifacts !== undefined){
let data1 = data.artifacts;
const _errs3 = errors;
if(errors === _errs3){
if(Array.isArray(data1)){
var valid1 = true;
const len0 = data1.length;
for(let i0=0; i0<len0; i0++){
const _errs5 = errors;
if(!(validate64(data1[i0], {instancePath:instancePath+"/artifacts/" + i0,parentData:data1,parentDataProperty:i0,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate64.errors : vErrors.concat(validate64.errors);
errors = vErrors.length;
}
var valid1 = _errs5 === errors;
if(!valid1){
break;
}
}
}
else {
validate154.errors = [{instancePath:instancePath+"/artifacts",schemaPath:"#/properties/artifacts/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.configuration !== undefined){
const _errs6 = errors;
if(!(validate99(data.configuration, {instancePath:instancePath+"/configuration",parentData:data,parentDataProperty:"configuration",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate99.errors : vErrors.concat(validate99.errors);
errors = vErrors.length;
}
var valid0 = _errs6 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_ms !== undefined){
let data4 = data.created_ms;
const _errs7 = errors;
if(!(((typeof data4 == "number") && (!(data4 % 1) && !isNaN(data4))) && (isFinite(data4)))){
validate154.errors = [{instancePath:instancePath+"/created_ms",schemaPath:"#/properties/created_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.deadline_ms !== undefined){
let data5 = data.deadline_ms;
const _errs9 = errors;
if(!(((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5))) && (isFinite(data5)))){
validate154.errors = [{instancePath:instancePath+"/deadline_ms",schemaPath:"#/properties/deadline_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs11 = errors;
if(typeof data.id !== "string"){
validate154.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.pause_reason !== undefined){
let data7 = data.pause_reason;
const _errs13 = errors;
if((typeof data7 !== "string") && (data7 !== null)){
validate154.errors = [{instancePath:instancePath+"/pause_reason",schemaPath:"#/properties/pause_reason/type",keyword:"type",params:{type: schema45.properties.pause_reason.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs13 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.request !== undefined){
const _errs15 = errors;
if(!(validate79(data.request, {instancePath:instancePath+"/request",parentData:data,parentDataProperty:"request",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate79.errors : vErrors.concat(validate79.errors);
errors = vErrors.length;
}
var valid0 = _errs15 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.status !== undefined){
const _errs16 = errors;
if(typeof data.status !== "string"){
validate154.errors = [{instancePath:instancePath+"/status",schemaPath:"#/properties/status/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs16 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.submission_limit !== undefined){
let data10 = data.submission_limit;
const _errs18 = errors;
if(!(((typeof data10 == "number") && (!(data10 % 1) && !isNaN(data10))) && (isFinite(data10)))){
validate154.errors = [{instancePath:instancePath+"/submission_limit",schemaPath:"#/properties/submission_limit/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs18){
if((typeof data10 == "number") && (isFinite(data10))){
if(data10 > 65535 || isNaN(data10)){
validate154.errors = [{instancePath:instancePath+"/submission_limit",schemaPath:"#/properties/submission_limit/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data10 < 0 || isNaN(data10)){
validate154.errors = [{instancePath:instancePath+"/submission_limit",schemaPath:"#/properties/submission_limit/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs18 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.submissions !== undefined){
let data11 = data.submissions;
const _errs20 = errors;
if(!(((typeof data11 == "number") && (!(data11 % 1) && !isNaN(data11))) && (isFinite(data11)))){
validate154.errors = [{instancePath:instancePath+"/submissions",schemaPath:"#/properties/submissions/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
if(errors === _errs20){
if((typeof data11 == "number") && (isFinite(data11))){
if(data11 > 65535 || isNaN(data11)){
validate154.errors = [{instancePath:instancePath+"/submissions",schemaPath:"#/properties/submissions/maximum",keyword:"maximum",params:{comparison: "<=", limit: 65535},message:"must be <= 65535"}];
return false;
}
else {
if(data11 < 0 || isNaN(data11)){
validate154.errors = [{instancePath:instancePath+"/submissions",schemaPath:"#/properties/submissions/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
}
var valid0 = _errs20 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.tasks !== undefined){
let data12 = data.tasks;
const _errs22 = errors;
if(errors === _errs22){
if(Array.isArray(data12)){
var valid2 = true;
const len1 = data12.length;
for(let i1=0; i1<len1; i1++){
const _errs24 = errors;
if(!(validate113(data12[i1], {instancePath:instancePath+"/tasks/" + i1,parentData:data12,parentDataProperty:i1,rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate113.errors : vErrors.concat(validate113.errors);
errors = vErrors.length;
}
var valid2 = _errs24 === errors;
if(!valid2){
break;
}
}
}
else {
validate154.errors = [{instancePath:instancePath+"/tasks",schemaPath:"#/properties/tasks/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs22 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
}
}
}
}
}
}
}
else {
validate154.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate154.errors = vErrors;
return errors === 0;
}
validate154.evaluated = {"props":{"allow_incomplete":true,"artifacts":true,"configuration":true,"created_ms":true,"deadline_ms":true,"id":true,"pause_reason":true,"request":true,"status":true,"submission_limit":true,"submissions":true,"tasks":true},"dynamicProps":false,"dynamicItems":false};

exports.run_accepted = validate159;
const schema48 = {"properties":{"result_url":{"type":"string"},"run_id":{"type":"string"},"run_url":{"type":"string"},"status":{"type":"string"}},"required":["run_id","status","run_url","result_url"],"type":"object"};

function validate159(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate159.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.run_id === undefined) && (missing0 = "run_id")) || ((data.status === undefined) && (missing0 = "status"))) || ((data.run_url === undefined) && (missing0 = "run_url"))) || ((data.result_url === undefined) && (missing0 = "result_url"))){
validate159.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.result_url !== undefined){
const _errs1 = errors;
if(typeof data.result_url !== "string"){
validate159.errors = [{instancePath:instancePath+"/result_url",schemaPath:"#/properties/result_url/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_id !== undefined){
const _errs3 = errors;
if(typeof data.run_id !== "string"){
validate159.errors = [{instancePath:instancePath+"/run_id",schemaPath:"#/properties/run_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.run_url !== undefined){
const _errs5 = errors;
if(typeof data.run_url !== "string"){
validate159.errors = [{instancePath:instancePath+"/run_url",schemaPath:"#/properties/run_url/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.status !== undefined){
const _errs7 = errors;
if(typeof data.status !== "string"){
validate159.errors = [{instancePath:instancePath+"/status",schemaPath:"#/properties/status/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
else {
validate159.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate159.errors = vErrors;
return errors === 0;
}
validate159.evaluated = {"props":{"result_url":true,"run_id":true,"run_url":true,"status":true},"dynamicProps":false,"dynamicItems":false};

exports.service_status = validate160;
const schema49 = {"properties":{"instance_id":{"type":["string","null"]},"pid":{"format":"uint32","minimum":0,"type":["integer","null"]},"runtime":{"type":"string"},"state":{"$ref":"#/$defs/ServiceState"},"version":{"type":"string"}},"required":["state","runtime","version"],"type":"object"};
const schema50 = {"enum":["running","stopping","stopped"],"type":"string"};

function validate121(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate121.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(typeof data !== "string"){
validate121.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
if(!(((data === "running") || (data === "stopping")) || (data === "stopped"))){
validate121.errors = [{instancePath,schemaPath:"#/enum",keyword:"enum",params:{allowedValues: schema50.enum},message:"must be equal to one of the allowed values"}];
return false;
}
validate121.errors = vErrors;
return errors === 0;
}
validate121.evaluated = {"dynamicProps":false,"dynamicItems":false};


function validate160(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate160.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((((data.state === undefined) && (missing0 = "state")) || ((data.runtime === undefined) && (missing0 = "runtime"))) || ((data.version === undefined) && (missing0 = "version"))){
validate160.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.instance_id !== undefined){
let data0 = data.instance_id;
const _errs1 = errors;
if((typeof data0 !== "string") && (data0 !== null)){
validate160.errors = [{instancePath:instancePath+"/instance_id",schemaPath:"#/properties/instance_id/type",keyword:"type",params:{type: schema49.properties.instance_id.type},message:"must be string,null"}];
return false;
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.pid !== undefined){
let data1 = data.pid;
const _errs3 = errors;
if((!(((typeof data1 == "number") && (!(data1 % 1) && !isNaN(data1))) && (isFinite(data1)))) && (data1 !== null)){
validate160.errors = [{instancePath:instancePath+"/pid",schemaPath:"#/properties/pid/type",keyword:"type",params:{type: schema49.properties.pid.type},message:"must be integer,null"}];
return false;
}
if(errors === _errs3){
if((typeof data1 == "number") && (isFinite(data1))){
if(data1 < 0 || isNaN(data1)){
validate160.errors = [{instancePath:instancePath+"/pid",schemaPath:"#/properties/pid/minimum",keyword:"minimum",params:{comparison: ">=", limit: 0},message:"must be >= 0"}];
return false;
}
}
}
var valid0 = _errs3 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.runtime !== undefined){
const _errs5 = errors;
if(typeof data.runtime !== "string"){
validate160.errors = [{instancePath:instancePath+"/runtime",schemaPath:"#/properties/runtime/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.state !== undefined){
const _errs7 = errors;
if(!(validate121(data.state, {instancePath:instancePath+"/state",parentData:data,parentDataProperty:"state",rootData,dynamicAnchors}))){
vErrors = vErrors === null ? validate121.errors : vErrors.concat(validate121.errors);
errors = vErrors.length;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.version !== undefined){
const _errs8 = errors;
if(typeof data.version !== "string"){
validate160.errors = [{instancePath:instancePath+"/version",schemaPath:"#/properties/version/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs8 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate160.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate160.errors = vErrors;
return errors === 0;
}
validate160.evaluated = {"props":{"instance_id":true,"pid":true,"runtime":true,"state":true,"version":true},"dynamicProps":false,"dynamicItems":false};

exports.service_stop_request = validate162;
const schema51 = {"additionalProperties":false,"properties":{"instance_id":{"type":"string"}},"required":["instance_id"],"type":"object"};

function validate162(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate162.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if((data.instance_id === undefined) && (missing0 = "instance_id")){
validate162.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
const _errs1 = errors;
for(const key0 in data){
if(!(key0 === "instance_id")){
validate162.errors = [{instancePath,schemaPath:"#/additionalProperties",keyword:"additionalProperties",params:{additionalProperty: key0},message:"must NOT have additional properties"}];
return false;
break;
}
}
if(_errs1 === errors){
if(data.instance_id !== undefined){
if(typeof data.instance_id !== "string"){
validate162.errors = [{instancePath:instancePath+"/instance_id",schemaPath:"#/properties/instance_id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
}
}
}
}
else {
validate162.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate162.errors = vErrors;
return errors === 0;
}
validate162.evaluated = {"props":true,"dynamicProps":false,"dynamicItems":false};

exports.token_metadata = validate163;

function validate163(data, {instancePath="", parentData, parentDataProperty, rootData=data, dynamicAnchors={}}={}){
let vErrors = null;
let errors = 0;
const evaluated0 = validate163.evaluated;
if(evaluated0.dynamicProps){
evaluated0.props = undefined;
}
if(evaluated0.dynamicItems){
evaluated0.items = undefined;
}
if(errors === 0){
if(data && typeof data == "object" && !Array.isArray(data)){
let missing0;
if(((((data.id === undefined) && (missing0 = "id")) || ((data.name === undefined) && (missing0 = "name"))) || ((data.account_ids === undefined) && (missing0 = "account_ids"))) || ((data.created_ms === undefined) && (missing0 = "created_ms"))){
validate163.errors = [{instancePath,schemaPath:"#/required",keyword:"required",params:{missingProperty: missing0},message:"must have required property '"+missing0+"'"}];
return false;
}
else {
if(data.account_ids !== undefined){
let data0 = data.account_ids;
const _errs1 = errors;
if(errors === _errs1){
if(Array.isArray(data0)){
var valid1 = true;
const len0 = data0.length;
for(let i0=0; i0<len0; i0++){
const _errs3 = errors;
if(typeof data0[i0] !== "string"){
validate163.errors = [{instancePath:instancePath+"/account_ids/" + i0,schemaPath:"#/properties/account_ids/items/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid1 = _errs3 === errors;
if(!valid1){
break;
}
}
}
else {
validate163.errors = [{instancePath:instancePath+"/account_ids",schemaPath:"#/properties/account_ids/type",keyword:"type",params:{type: "array"},message:"must be array"}];
return false;
}
}
var valid0 = _errs1 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.created_ms !== undefined){
let data2 = data.created_ms;
const _errs5 = errors;
if(!(((typeof data2 == "number") && (!(data2 % 1) && !isNaN(data2))) && (isFinite(data2)))){
validate163.errors = [{instancePath:instancePath+"/created_ms",schemaPath:"#/properties/created_ms/type",keyword:"type",params:{type: "integer"},message:"must be integer"}];
return false;
}
var valid0 = _errs5 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.id !== undefined){
const _errs7 = errors;
if(typeof data.id !== "string"){
validate163.errors = [{instancePath:instancePath+"/id",schemaPath:"#/properties/id/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs7 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.name !== undefined){
const _errs9 = errors;
if(typeof data.name !== "string"){
validate163.errors = [{instancePath:instancePath+"/name",schemaPath:"#/properties/name/type",keyword:"type",params:{type: "string"},message:"must be string"}];
return false;
}
var valid0 = _errs9 === errors;
}
else {
var valid0 = true;
}
if(valid0){
if(data.revoked_ms !== undefined){
let data5 = data.revoked_ms;
const _errs11 = errors;
if((!(((typeof data5 == "number") && (!(data5 % 1) && !isNaN(data5))) && (isFinite(data5)))) && (data5 !== null)){
validate163.errors = [{instancePath:instancePath+"/revoked_ms",schemaPath:"#/properties/revoked_ms/type",keyword:"type",params:{type: schema38.properties.revoked_ms.type},message:"must be integer,null"}];
return false;
}
var valid0 = _errs11 === errors;
}
else {
var valid0 = true;
}
}
}
}
}
}
}
else {
validate163.errors = [{instancePath,schemaPath:"#/type",keyword:"type",params:{type: "object"},message:"must be object"}];
return false;
}
}
validate163.errors = vErrors;
return errors === 0;
}
validate163.evaluated = {"props":{"account_ids":true,"created_ms":true,"id":true,"name":true,"revoked_ms":true},"dynamicProps":false,"dynamicItems":false};
